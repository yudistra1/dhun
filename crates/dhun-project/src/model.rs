//! The serializable project data model (SPEC §3.6).
//!
//! These types are plain data with `serde` derives. They deliberately do **not**
//! reference `dhun-dsp` — the project layer depends only on `dhun-core` (SPEC §6)
//! — so instrument settings are mirrored here as spec types and translated into
//! DSP types by `dhun-engine`.

use serde::{Deserialize, Serialize};

/// The project format version this build writes and can read up to.
pub const CURRENT_FORMAT_VERSION: u32 = 1;

/// A whole Dhun project: transport, instruments, patterns, arrangement, master.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Serialization format version, for forward-migration (SPEC §3.6).
    pub format_version: u32,
    /// Human-readable project name.
    pub name: String,
    /// Sample rate in hertz (44 100 or 48 000).
    pub sample_rate: u32,
    /// Tempo in beats per minute.
    pub tempo: f64,
    /// Steps per beat for the step sequencer (4 = sixteenth-note grid).
    pub steps_per_beat: u32,
    /// Instrument channels.
    pub channels: Vec<Channel>,
    /// Patterns, referenced by the song arrangement.
    pub patterns: Vec<Pattern>,
    /// Linear song arrangement: pattern ids played in order (SPEC §3.2).
    pub song: Vec<u64>,
    /// Recorded audio tracks placed on the global timeline (SPEC §3.4).
    ///
    /// `#[serde(default)]` keeps projects written before audio tracks existed
    /// loadable.
    #[serde(default)]
    pub audio_tracks: Vec<AudioTrack>,
    /// Master channel settings.
    pub master: Master,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            name: "Untitled".to_string(),
            sample_rate: 48_000,
            tempo: 120.0,
            steps_per_beat: 4,
            channels: Vec::new(),
            patterns: Vec::new(),
            song: Vec::new(),
            audio_tracks: Vec::new(),
            master: Master::default(),
        }
    }
}

/// A track of recorded audio clips with its own mixer strip (SPEC §3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    /// Stable track id.
    pub id: u64,
    /// Human-readable name.
    pub name: String,
    /// Mixer settings for the track.
    pub mixer: MixerStrip,
    /// Clips placed on this track.
    pub clips: Vec<AudioClip>,
}

/// A recorded audio clip, stored as a WAV in the project's `audio/` folder and
/// placed on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioClip {
    /// Source file, relative to the project folder (e.g. `audio/take1.wav`).
    pub source: String,
    /// Placement on the global timeline, in ticks.
    pub start_tick: u32,
    /// Frames trimmed from the start of the source. Also used for latency
    /// compensation: set to the input latency so the first played frame lands
    /// on the beat (SPEC §3.4).
    #[serde(default)]
    pub offset_frames: u32,
    /// Clip length in frames; `0` plays to the end of the source.
    #[serde(default)]
    pub length_frames: u32,
    /// Clip gain in decibels.
    #[serde(default)]
    pub gain_db: f32,
}

/// An instrument channel with its own mixer strip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Stable channel id, referenced by pattern lanes.
    pub id: u64,
    /// Human-readable name.
    pub name: String,
    /// The instrument this channel plays.
    pub instrument: Instrument,
    /// This channel's mixer settings.
    pub mixer: MixerStrip,
}

/// The instrument a channel plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Instrument {
    /// A single drum voice, triggered by step lanes.
    Drum {
        /// Which drum this channel is.
        drum: DrumKind,
    },
    /// A polyphonic subtractive synth, played by note lanes.
    Synth {
        /// The synth patch.
        params: SynthSpec,
    },
}

/// The built-in drum voices (mirrors `dhun_dsp::DrumType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrumKind {
    /// Bass drum.
    Kick,
    /// Snare drum.
    Snare,
    /// Hand clap.
    Clap,
    /// Closed hi-hat.
    ClosedHat,
    /// Open hi-hat.
    OpenHat,
    /// Tom.
    Tom,
}

/// Oscillator waveform (mirrors `dhun_dsp::Waveform`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveformSpec {
    /// Sine.
    Sine,
    /// Sawtooth.
    Saw,
    /// Square.
    Square,
    /// Triangle.
    Triangle,
}

/// A subtractive-synth patch (mirrors `dhun_dsp::SynthParams`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SynthSpec {
    /// Oscillator 1 waveform.
    pub osc1: WaveformSpec,
    /// Oscillator 2 waveform.
    pub osc2: WaveformSpec,
    /// Oscillator 2 detune in cents.
    pub detune_cents: f32,
    /// Oscillator balance (0 = osc1 only, 1 = osc2 only).
    pub mix: f32,
    /// Low-pass cutoff in hertz.
    pub cutoff: f32,
    /// Low-pass resonance (Q).
    pub resonance: f32,
    /// Amp envelope.
    pub amp: AdsrSpec,
}

impl Default for SynthSpec {
    fn default() -> Self {
        Self {
            osc1: WaveformSpec::Saw,
            osc2: WaveformSpec::Saw,
            detune_cents: 7.0,
            mix: 0.5,
            cutoff: 8_000.0,
            resonance: 0.707,
            amp: AdsrSpec::default(),
        }
    }
}

/// An ADSR envelope spec (mirrors `dhun_dsp::AdsrParams`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdsrSpec {
    /// Attack time in seconds.
    pub attack: f32,
    /// Decay time in seconds.
    pub decay: f32,
    /// Sustain level in `[0, 1]`.
    pub sustain: f32,
    /// Release time in seconds.
    pub release: f32,
}

impl Default for AdsrSpec {
    fn default() -> Self {
        Self {
            attack: 0.005,
            decay: 0.1,
            sustain: 0.7,
            release: 0.2,
        }
    }
}

/// A built-in insert effect and its parameters (SPEC §3.5).
///
/// Mirrors the `dhun-dsp` effect params (the project layer does not depend on
/// `dhun-dsp`); `dhun-engine` translates these into DSP processors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum EffectSpec {
    /// A gain stage, in decibels.
    Gain {
        /// Gain in dB.
        gain_db: f32,
    },
    /// A 3-band EQ (low shelf, mid peak, high shelf); gains in dB.
    Eq {
        /// Low-shelf gain in dB.
        low_db: f32,
        /// Mid peak gain in dB.
        mid_db: f32,
        /// High-shelf gain in dB.
        high_db: f32,
    },
    /// A feed-forward compressor.
    Compressor {
        /// Threshold in dBFS.
        threshold_db: f32,
        /// Ratio (>= 1).
        ratio: f32,
        /// Attack time in seconds.
        attack: f32,
        /// Release time in seconds.
        release: f32,
        /// Make-up gain in dB.
        makeup_db: f32,
    },
    /// A reverb. `room_size`, `damping`, `wet`, `dry` are in `[0, 1]`.
    Reverb {
        /// Room size.
        room_size: f32,
        /// High-frequency damping.
        damping: f32,
        /// Wet (processed) level.
        wet: f32,
        /// Dry (unprocessed) level.
        dry: f32,
    },
}

impl EffectSpec {
    /// A default gain insert (0 dB).
    #[must_use]
    pub fn gain() -> Self {
        EffectSpec::Gain { gain_db: 0.0 }
    }

    /// A default flat 3-band EQ.
    #[must_use]
    pub fn eq() -> Self {
        EffectSpec::Eq {
            low_db: 0.0,
            mid_db: 0.0,
            high_db: 0.0,
        }
    }

    /// A default compressor (−18 dB, 4:1).
    #[must_use]
    pub fn compressor() -> Self {
        EffectSpec::Compressor {
            threshold_db: -18.0,
            ratio: 4.0,
            attack: 0.005,
            release: 0.1,
            makeup_db: 0.0,
        }
    }

    /// A default reverb.
    #[must_use]
    pub fn reverb() -> Self {
        EffectSpec::Reverb {
            room_size: 0.5,
            damping: 0.5,
            wet: 0.3,
            dry: 0.7,
        }
    }

    /// A short label for the effect kind.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            EffectSpec::Gain { .. } => "Gain",
            EffectSpec::Eq { .. } => "EQ",
            EffectSpec::Compressor { .. } => "Comp",
            EffectSpec::Reverb { .. } => "Reverb",
        }
    }
}

/// A per-channel mixer strip (SPEC §3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerStrip {
    /// Volume in decibels.
    pub gain_db: f32,
    /// Pan in `[-1, 1]` (−1 = left, 0 = center, +1 = right).
    pub pan: f32,
    /// Muted if true.
    pub mute: bool,
    /// Soloed if true.
    pub solo: bool,
    /// Insert effect chain, applied in order to the (mono) channel signal.
    #[serde(default)]
    pub effects: Vec<EffectSpec>,
}

impl Default for MixerStrip {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            effects: Vec::new(),
        }
    }
}

/// The master channel (SPEC §3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Master {
    /// Master volume in decibels.
    pub gain_db: f32,
    /// Insert effect chain, applied (per side) to the stereo master bus.
    #[serde(default)]
    pub effects: Vec<EffectSpec>,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            effects: Vec::new(),
        }
    }
}

/// A pattern: step lanes for drums and note lanes for synths (SPEC §3.2–3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    /// Stable pattern id, referenced by the song.
    pub id: u64,
    /// Human-readable name.
    pub name: String,
    /// Number of steps in the pattern (8, 16, or 32).
    pub steps: u32,
    /// Drum step lanes.
    pub drum_lanes: Vec<DrumLane>,
    /// Synth note lanes.
    pub note_lanes: Vec<NoteLane>,
}

/// A step lane for one drum channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumLane {
    /// The channel this lane drives.
    pub channel: u64,
    /// One entry per pattern step.
    pub steps: Vec<Step>,
}

/// A single sequencer step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Whether the step triggers.
    pub on: bool,
    /// Trigger velocity in `[0, 1]`.
    pub velocity: f32,
}

/// A note lane for one synth channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteLane {
    /// The channel this lane drives.
    pub channel: u64,
    /// Notes within the pattern.
    pub notes: Vec<Note>,
}

/// A piano-roll note, timed in ticks from the pattern start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// MIDI pitch (0–127).
    pub pitch: u8,
    /// Start offset in ticks from the pattern start.
    pub start_tick: u32,
    /// Duration in ticks.
    pub length_ticks: u32,
    /// Velocity in `[0, 1]`.
    pub velocity: f32,
}
