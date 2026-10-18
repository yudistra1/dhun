//! `dhun` — the headless CLI (SPEC §3.7).
//!
//! Every engine feature should be reachable here so agents can test without a
//! GUI. v0.1 exposes `render`, `info`, and `validate`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dhun_engine::{render_offline, RenderRange, RenderedAudio};
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
    }
}

fn cmd_render(project_path: &Path, out: &Path, range: Option<&str>) -> Result<()> {
    let project = Project::load(project_path)
        .with_context(|| format!("loading project {}", project_path.display()))?;
    let range = range.map(parse_range).transpose()?;

    let audio = render_offline(&project, range).context("rendering project")?;
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
