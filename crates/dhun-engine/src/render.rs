//! Offline, deterministic render of a [`Project`] to interleaved stereo audio
//! (SPEC §3.6, §5 "Determinism").
//!
//! This path runs off the audio thread, so it may allocate freely; the DSP it
//! drives is the same allocation-free code used in real time.

use std::f32::consts::FRAC_PI_2;

use dhun_core::{Bpm, SampleRate, TempoMap, Ticks, PPQ};
use dhun_dsp::{db_to_lin, AdsrParams, DrumKit, DrumType, Synth, SynthParams, Waveform};
use dhun_project::{AdsrSpec, DrumKind, Instrument, Note, Project, SynthSpec, WaveformSpec};

use crate::error::EngineError;

/// Extra time rendered after the song ends so note/drum tails are not cut off.
const TAIL_SECONDS: f64 = 2.0;

/// A render range in whole bars (4/4). End is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderRange {
    /// First bar to include (0-based).
    pub start_bar: u32,
    /// One past the last bar to include.
    pub end_bar: u32,
}

/// The result of an offline render: interleaved stereo `f32` in `[-1, 1]`.
#[derive(Debug, Clone)]
pub struct RenderedAudio {
    /// Sample rate in hertz.
    pub sample_rate: u32,
    /// Number of stereo frames.
    pub frames: usize,
    /// Interleaved L/R samples, length `frames * 2`.
    pub interleaved: Vec<f32>,
}

impl RenderedAudio {
    /// Number of audio channels (always stereo in v0.1).
    pub const CHANNELS: u16 = 2;

    /// Converts the float samples to 16-bit PCM, clamping to `[-1, 1]`.
    ///
    /// Centralized here so the CLI's WAV output and the golden test quantize
    /// identically.
    #[must_use]
    pub fn to_i16(&self) -> Vec<i16> {
        self.interleaved
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
            .collect()
    }
}

/// A scheduled instrument action, timed in output samples.
enum Event {
    Drum { at: usize, velocity: f32 },
    NoteOn { at: usize, pitch: u8, velocity: f32 },
    NoteOff { at: usize, pitch: u8 },
}

impl Event {
    fn at(&self) -> usize {
        match *self {
            Event::Drum { at, .. } | Event::NoteOn { at, .. } | Event::NoteOff { at, .. } => at,
        }
    }
}

/// An instrument voice backing one project channel.
///
/// `DrumKit` holds all six drum voices inline, so it is boxed to keep the enum
/// variants a similar size.
enum Voice {
    Drum { kit: Box<DrumKit>, drum: DrumType },
    Synth(Synth),
}

impl Voice {
    fn apply(&mut self, event: &Event) {
        match self {
            Voice::Drum { kit, drum } => {
                if let Event::Drum { velocity, .. } = event {
                    kit.trigger(*drum, *velocity);
                }
            }
            Voice::Synth(synth) => match event {
                Event::NoteOn {
                    pitch, velocity, ..
                } => synth.note_on(*pitch, *velocity),
                Event::NoteOff { pitch, .. } => synth.note_off(*pitch),
                Event::Drum { .. } => {}
            },
        }
    }

    fn tick(&mut self) -> f32 {
        match self {
            Voice::Drum { kit, .. } => kit.tick(),
            Voice::Synth(synth) => synth.tick(),
        }
    }
}

/// Renders a validated project offline.
///
/// `range` limits output to a bar window; `None` renders the whole song plus a
/// short tail.
///
/// # Errors
/// Returns [`EngineError::Project`] if the project fails validation and
/// [`EngineError::BadRange`] if `range` is empty or inverted.
pub fn render_offline(
    project: &Project,
    range: Option<RenderRange>,
) -> Result<RenderedAudio, EngineError> {
    project.validate()?;

    if let Some(r) = range {
        if r.end_bar <= r.start_bar {
            return Err(EngineError::BadRange {
                start: r.start_bar,
                end: r.end_bar,
            });
        }
    }

    // Validation guarantees these succeed.
    let sample_rate = SampleRate::new(project.sample_rate)
        .expect("sample rate validated")
        .get();
    let tempo = Bpm::new(project.tempo).expect("tempo validated");
    let map = TempoMap::new(
        tempo,
        SampleRate::new(sample_rate).expect("sample rate validated"),
    );

    let ticks_per_step = PPQ / project.steps_per_beat.max(1);
    let tick_to_sample = |tick: u32| -> usize {
        let s = map.ticks_to_samples(Ticks::new(i64::from(tick))).get();
        usize::try_from(s).unwrap_or(0)
    };

    // Total musical length of the arranged song, in ticks.
    let mut song_ticks: u64 = 0;
    for &pid in &project.song {
        if let Some(pat) = project.pattern(pid) {
            song_ticks += u64::from(pat.steps) * u64::from(ticks_per_step);
        }
    }
    let song_samples = tick_to_sample(u32::try_from(song_ticks).unwrap_or(u32::MAX));
    let tail = (TAIL_SECONDS * f64::from(sample_rate)) as usize;
    let total_frames = song_samples + tail;

    // Render each channel to its own mono buffer.
    let mut left = vec![0.0f32; total_frames];
    let mut right = vec![0.0f32; total_frames];
    let any_solo = project.channels.iter().any(|c| c.mixer.solo);

    for channel in &project.channels {
        if channel.mixer.mute || (any_solo && !channel.mixer.solo) {
            continue;
        }

        let mut voice = build_voice(&channel.instrument, sample_rate as f32);
        let mut events = collect_events(project, channel.id, ticks_per_step, &tick_to_sample);
        events.retain(|e| e.at() < total_frames);
        events.sort_by_key(Event::at);

        let mono = render_voice(&mut voice, &events, total_frames);

        // Pan law: constant-power, pan in [-1, 1].
        let gain = db_to_lin(channel.mixer.gain_db);
        let theta = (channel.mixer.pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * FRAC_PI_2;
        let (lg, rg) = (theta.cos() * gain, theta.sin() * gain);
        for (i, &s) in mono.iter().enumerate() {
            left[i] += s * lg;
            right[i] += s * rg;
        }
    }

    // Master gain and interleave.
    let master = db_to_lin(project.master.gain_db);
    let mut interleaved = Vec::with_capacity(total_frames * 2);
    for i in 0..total_frames {
        interleaved.push(left[i] * master);
        interleaved.push(right[i] * master);
    }

    let mut audio = RenderedAudio {
        sample_rate,
        frames: total_frames,
        interleaved,
    };

    if let Some(r) = range {
        slice_to_range(&mut audio, r, &map);
    }

    Ok(audio)
}

/// Keeps only the samples within the requested bar window.
fn slice_to_range(audio: &mut RenderedAudio, range: RenderRange, map: &TempoMap) {
    let bar_ticks = 4 * PPQ; // 4/4
    let bar_to_frame = |bar: u32| -> usize {
        let s = map
            .ticks_to_samples(Ticks::new(i64::from(bar) * i64::from(bar_ticks)))
            .get();
        usize::try_from(s).unwrap_or(0)
    };
    let start = bar_to_frame(range.start_bar).min(audio.frames);
    let end = bar_to_frame(range.end_bar).min(audio.frames);
    audio.interleaved = audio.interleaved[start * 2..end * 2].to_vec();
    audio.frames = end - start;
}

fn build_voice(instrument: &Instrument, sample_rate: f32) -> Voice {
    match instrument {
        Instrument::Drum { drum } => Voice::Drum {
            kit: Box::new(DrumKit::new(sample_rate)),
            drum: map_drum(*drum),
        },
        Instrument::Synth { params } => Voice::Synth(Synth::new(sample_rate, map_synth(*params))),
    }
}

/// Collects a channel's scheduled events across the whole song timeline.
fn collect_events(
    project: &Project,
    channel_id: u64,
    ticks_per_step: u32,
    tick_to_sample: &impl Fn(u32) -> usize,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut offset_ticks: u32 = 0;

    for &pid in &project.song {
        let Some(pat) = project.pattern(pid) else {
            continue;
        };
        for lane in &pat.drum_lanes {
            if lane.channel != channel_id {
                continue;
            }
            for (idx, step) in lane.steps.iter().enumerate() {
                if step.on {
                    let tick = offset_ticks + idx as u32 * ticks_per_step;
                    events.push(Event::Drum {
                        at: tick_to_sample(tick),
                        velocity: step.velocity,
                    });
                }
            }
        }
        for lane in &pat.note_lanes {
            if lane.channel != channel_id {
                continue;
            }
            for note in &lane.notes {
                push_note_events(&mut events, note, offset_ticks, tick_to_sample);
            }
        }
        offset_ticks += pat.steps * ticks_per_step;
    }
    events
}

fn push_note_events(
    events: &mut Vec<Event>,
    note: &Note,
    offset_ticks: u32,
    tick_to_sample: &impl Fn(u32) -> usize,
) {
    let on = offset_ticks + note.start_tick;
    let off = on + note.length_ticks;
    events.push(Event::NoteOn {
        at: tick_to_sample(on),
        pitch: note.pitch,
        velocity: note.velocity,
    });
    events.push(Event::NoteOff {
        at: tick_to_sample(off),
        pitch: note.pitch,
    });
}

/// Renders one voice to a mono buffer, applying its events in time order.
fn render_voice(voice: &mut Voice, events: &[Event], total_frames: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; total_frames];
    let mut ei = 0;
    for (i, out) in buf.iter_mut().enumerate() {
        while ei < events.len() && events[ei].at() == i {
            voice.apply(&events[ei]);
            ei += 1;
        }
        *out = voice.tick();
    }
    buf
}

fn map_drum(kind: DrumKind) -> DrumType {
    match kind {
        DrumKind::Kick => DrumType::Kick,
        DrumKind::Snare => DrumType::Snare,
        DrumKind::Clap => DrumType::Clap,
        DrumKind::ClosedHat => DrumType::ClosedHat,
        DrumKind::OpenHat => DrumType::OpenHat,
        DrumKind::Tom => DrumType::Tom,
    }
}

fn map_waveform(w: WaveformSpec) -> Waveform {
    match w {
        WaveformSpec::Sine => Waveform::Sine,
        WaveformSpec::Saw => Waveform::Saw,
        WaveformSpec::Square => Waveform::Square,
        WaveformSpec::Triangle => Waveform::Triangle,
    }
}

fn map_adsr(a: AdsrSpec) -> AdsrParams {
    AdsrParams {
        attack: a.attack,
        decay: a.decay,
        sustain: a.sustain,
        release: a.release,
    }
}

fn map_synth(spec: SynthSpec) -> SynthParams {
    SynthParams {
        osc1: map_waveform(spec.osc1),
        osc2: map_waveform(spec.osc2),
        detune_cents: spec.detune_cents,
        mix: spec.mix,
        cutoff: spec.cutoff,
        resonance: spec.resonance,
        amp: map_adsr(spec.amp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dhun_project::{Channel, DrumLane, MixerStrip, Pattern, Step};

    fn one_kick_project() -> Project {
        Project {
            sample_rate: 48_000,
            tempo: 120.0,
            steps_per_beat: 4,
            channels: vec![Channel {
                id: 1,
                name: "Kick".to_string(),
                instrument: Instrument::Drum {
                    drum: DrumKind::Kick,
                },
                mixer: MixerStrip::default(),
            }],
            patterns: vec![Pattern {
                id: 1,
                name: "A".to_string(),
                steps: 4,
                drum_lanes: vec![DrumLane {
                    channel: 1,
                    steps: vec![
                        Step {
                            on: true,
                            velocity: 1.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                    ],
                }],
                note_lanes: vec![],
            }],
            song: vec![1],
            ..Project::default()
        }
    }

    #[test]
    fn renders_stereo_and_finite() {
        let audio = render_offline(&one_kick_project(), None).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.interleaved.len(), audio.frames * 2);
        assert!(audio.interleaved.iter().all(|s| s.is_finite()));
        assert!(audio.interleaved.iter().all(|s| s.abs() <= 1.5));
    }

    #[test]
    fn kick_on_beat_one_produces_early_energy() {
        let audio = render_offline(&one_kick_project(), None).unwrap();
        // The kick hits at frame 0; the first 10 ms should be clearly audible.
        let early_peak = audio.interleaved[..960 * 2]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(early_peak > 0.05, "kick onset too quiet: {early_peak}");
    }

    #[test]
    fn render_is_deterministic() {
        let p = one_kick_project();
        let a = render_offline(&p, None).unwrap();
        let b = render_offline(&p, None).unwrap();
        assert_eq!(a.interleaved, b.interleaved, "render not deterministic");
    }

    #[test]
    fn all_steps_off_is_silent() {
        let mut p = one_kick_project();
        p.patterns[0].drum_lanes[0]
            .steps
            .iter_mut()
            .for_each(|s| s.on = false);
        let audio = render_offline(&p, None).unwrap();
        assert!(
            audio.interleaved.iter().all(|&s| s == 0.0),
            "expected silence"
        );
    }

    #[test]
    fn muted_channel_is_silent() {
        let mut p = one_kick_project();
        p.channels[0].mixer.mute = true;
        let audio = render_offline(&p, None).unwrap();
        assert!(audio.interleaved.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn to_i16_clamps_and_scales() {
        let audio = RenderedAudio {
            sample_rate: 48_000,
            frames: 3,
            interleaved: vec![0.0, 1.0, -1.0, 2.0, -2.0, 0.5],
        };
        let pcm = audio.to_i16();
        assert_eq!(pcm[0], 0);
        assert_eq!(pcm[1], i16::MAX);
        assert_eq!(pcm[2], -i16::MAX);
        assert_eq!(pcm[3], i16::MAX, "clamps above 1.0");
        assert_eq!(pcm[4], -i16::MAX, "clamps below -1.0");
    }
}
