//! `dhun-dsp` — Dhun's own signal-processing building blocks and instruments.
//!
//! Everything here is pure and allocation-free on the process path: buffers and
//! voices are sized at construction, and per-sample `tick` methods never
//! allocate, lock, or do I/O (CLAUDE.md real-time rules). All built-in sounds
//! are synthesized here — Dhun ships no third-party samples (SPEC §3.2).
//!
//! Layers: [`oscillator`], [`envelope`], and [`filter`] are the primitives;
//! `synth` and `drums` are the instruments; `effects` holds the mixer
//! processors.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod drums;
pub mod effects;
pub mod envelope;
pub mod filter;
pub mod oscillator;
pub mod synth;
pub mod util;

pub use drums::{DrumKit, DrumType};
pub use effects::{Compressor, Effect, Gain, Reverb, ThreeBandEq, ThreeBandEqParams};
pub use envelope::{Adsr, AdsrParams, Stage};
pub use filter::SvfLowpass;
pub use oscillator::{Oscillator, Waveform};
pub use synth::{Synth, SynthParams, VOICES};
pub use util::{db_to_lin, lin_to_db, note_to_freq, NoiseRng};
