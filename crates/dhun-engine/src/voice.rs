//! Instrument voices and the actions that drive them, shared by the offline
//! renderer and the real-time player.

use dhun_dsp::{AdsrParams, DrumKit, DrumType, Synth, SynthParams, Waveform};
use dhun_project::{AdsrSpec, DrumKind, Instrument, SynthSpec, WaveformSpec};

/// A scheduled instrument action, independent of when it occurs.
///
/// `Copy` so it can be read out of an event list without borrowing across a
/// mutable voice access on the audio thread.
#[derive(Debug, Clone, Copy)]
pub(crate) enum EventKind {
    /// Trigger a drum at the given velocity.
    Drum { velocity: f32 },
    /// Start a synth note.
    NoteOn { pitch: u8, velocity: f32 },
    /// Release a synth note.
    NoteOff { pitch: u8 },
}

/// An instrument voice backing one project channel.
///
/// `DrumKit` holds all six drum voices inline, so it is boxed to keep the enum
/// variants a similar size.
pub(crate) enum Voice {
    /// A single drum voice within a kit.
    Drum {
        /// The kit (boxed; large).
        kit: Box<DrumKit>,
        /// Which drum this channel plays.
        drum: DrumType,
    },
    /// A polyphonic synth.
    Synth(Synth),
}

impl Voice {
    /// Applies an event. Allocation-free — safe on the audio thread.
    pub(crate) fn apply(&mut self, kind: &EventKind) {
        match self {
            Voice::Drum { kit, drum } => {
                if let EventKind::Drum { velocity } = kind {
                    kit.trigger(*drum, *velocity);
                }
            }
            Voice::Synth(synth) => match kind {
                EventKind::NoteOn { pitch, velocity } => synth.note_on(*pitch, *velocity),
                EventKind::NoteOff { pitch } => synth.note_off(*pitch),
                EventKind::Drum { .. } => {}
            },
        }
    }

    /// Advances one sample. Allocation-free.
    pub(crate) fn tick(&mut self) -> f32 {
        match self {
            Voice::Drum { kit, .. } => kit.tick(),
            Voice::Synth(synth) => synth.tick(),
        }
    }
}

/// Builds the voice for a project instrument at the given sample rate.
pub(crate) fn build_voice(instrument: &Instrument, sample_rate: f32) -> Voice {
    match instrument {
        Instrument::Drum { drum } => Voice::Drum {
            kit: Box::new(DrumKit::new(sample_rate)),
            drum: map_drum(*drum),
        },
        Instrument::Synth { params } => Voice::Synth(Synth::new(sample_rate, map_synth(*params))),
    }
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
