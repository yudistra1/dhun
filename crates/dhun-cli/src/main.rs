//! `dhun` — the headless CLI (SPEC §3.7).
//!
//! Every engine feature should be reachable here so agents can test without a
//! GUI. v0.1 exposes `render`, `info`, and `validate`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dhun_engine::{render_offline_with, AudioAssets, RenderRange, RenderedAudio};
use dhun_io::{Playback, Recorder};
use dhun_project::Project;

#[derive(Parser)]
#[command(name = "dhun", version, about = "Dhun headless engine CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a project to a WAV file (offline, deterministic).
    Render {
        /// Project folder or `project.json` path.
        project: PathBuf,
        /// Output WAV path.
        #[arg(short, long)]
        out: PathBuf,
        /// Optional bar range `start:end` (end exclusive), e.g. `0:4`.
        #[arg(long)]
        range: Option<String>,
    },
    /// Print a summary of a project.
    Info {
        /// Project folder or `project.json` path.
        project: PathBuf,
    },
    /// Validate a project file's integrity.
    Validate {
        /// Project folder or `project.json` path.
        project: PathBuf,
    },
    /// Play a project out the default audio device in real time.
    Play {
        /// Project folder or `project.json` path.
        project: PathBuf,
        /// Loop the song until interrupted.
        #[arg(long = "loop")]
        looping: bool,
    },
    /// Record the default audio input to a WAV file.
    Record {
        /// Output WAV path.
        #[arg(short, long)]
        out: PathBuf,
        /// Record for this many seconds; omit to record until you press Enter.
        #[arg(long)]
        seconds: Option<f32>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Render {
            project,
            out,
            range,
        } => cmd_render(&project, &out, range.as_deref()),
        Command::Info { project } => cmd_info(&project),
        Command::Validate { project } => cmd_validate(&project),
        Command::Play { project, looping } => cmd_play(&project, looping),
        Command::Record { out, seconds } => cmd_record(&out, seconds),
    }
}

/// The folder a project lives in (so relative clip paths resolve).
fn project_dir(project_path: &Path) -> PathBuf {
    if project_path.is_dir() {
        project_path.to_path_buf()
    } else {
        project_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }
}

fn cmd_render(project_path: &Path, out: &Path, range: Option<&str>) -> Result<()> {
    let project = Project::load(project_path)
        .with_context(|| format!("loading project {}", project_path.display()))?;
    let range = range.map(parse_range).transpose()?;

    let assets = AudioAssets::load_from_dir(&project, &project_dir(project_path))
        .context("loading audio clips")?;
    let audio = render_offline_with(&project, range, &assets).context("rendering project")?;
    write_wav(out, &audio).with_context(|| format!("writing {}", out.display()))?;

    let seconds = audio.frames as f64 / f64::from(audio.sample_rate);
    println!(
        "Rendered {} frames ({seconds:.2}s @ {} Hz) -> {}",
        audio.frames,
        audio.sample_rate,
        out.display()
    );
    Ok(())
}

fn cmd_info(project_path: &Path) -> Result<()> {
    let project = Project::load(project_path)
        .with_context(|| format!("loading project {}", project_path.display()))?;
    println!("Name:         {}", project.name);
    println!("Format:       v{}", project.format_version);
    println!("Sample rate:  {} Hz", project.sample_rate);
    println!("Tempo:        {} BPM", project.tempo);
    println!("Channels:     {}", project.channels.len());
    println!("Patterns:     {}", project.patterns.len());
    println!("Song length:  {} patterns", project.song.len());
    Ok(())
}

fn cmd_validate(project_path: &Path) -> Result<()> {
    match Project::load(project_path) {
        Ok(_) => {
            println!("OK: {} is valid", project_path.display());
            Ok(())
        }
        Err(e) => bail!("invalid project {}: {e}", project_path.display()),
    }
}

fn cmd_play(project_path: &Path, looping: bool) -> Result<()> {
    let project = Project::load(project_path)
        .with_context(|| format!("loading project {}", project_path.display()))?;
    let mut playback = Playback::start(&project).context("opening audio output device")?;

    println!(
        "Playing \"{}\" @ {} Hz{}. Press Ctrl-C to stop.",
        project.name,
        playback.sample_rate(),
        if looping { " (looping)" } else { "" }
    );
    playback.set_looping(looping);
    playback.play();

    if looping {
        loop {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    while !playback.is_finished() {
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("Done.");
    Ok(())
}

fn cmd_record(out: &Path, seconds: Option<f32>) -> Result<()> {
    let recorder = Recorder::start(out).context("opening audio input device")?;
    println!(
        "Recording @ {} Hz -> {}",
        recorder.sample_rate(),
        out.display()
    );

    if let Some(secs) = seconds {
        println!("Capturing for {secs:.1}s...");
        std::thread::sleep(Duration::from_secs_f32(secs.max(0.0)));
    } else {
        println!("Press Enter to stop...");
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("reading from stdin")?;
    }

    let recording = recorder.stop().context("finalizing recording")?;
    let duration = recording.frames as f64 / f64::from(recording.sample_rate.max(1));
    println!(
        "Recorded {} frames ({duration:.2}s, {} ch) -> {}",
        recording.frames,
        recording.channels,
        recording.path.display()
    );
    println!(
        "Add it to a project as an audio clip, e.g. {{ \"source\": \"{}\", \"start_tick\": 0 }}",
        recording
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("take.wav")
    );
    Ok(())
}

/// Parses a `start:end` bar range.
fn parse_range(spec: &str) -> Result<RenderRange> {
    let (a, b) = spec
        .split_once(':')
        .with_context(|| format!("range must be `start:end`, got {spec:?}"))?;
    let start_bar: u32 = a.trim().parse().context("range start")?;
    let end_bar: u32 = b.trim().parse().context("range end")?;
    if end_bar <= start_bar {
        bail!("range end ({end_bar}) must be greater than start ({start_bar})");
    }
    Ok(RenderRange { start_bar, end_bar })
}

/// Writes rendered audio as a 16-bit stereo WAV.
fn write_wav(path: &Path, audio: &RenderedAudio) -> Result<()> {
    let spec = hound::WavSpec {
        channels: RenderedAudio::CHANNELS,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in audio.to_i16() {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}
