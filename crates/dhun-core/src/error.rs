//! The shared error type returned by `dhun-core` constructors.

use thiserror::Error;

/// Errors produced when constructing core types from untrusted or out-of-range
/// values.
///
/// These are validation errors only: malformed input must always surface as an
/// `Err`, never a panic (SPEC §5, "Stability").
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CoreError {
    /// A tempo outside Dhun's supported range (SPEC §3.1: 40–240 BPM).
    #[error("tempo {value} BPM is out of range (must be {min}..={max})")]
    TempoOutOfRange {
        /// The rejected tempo.
        value: f64,
        /// The minimum allowed tempo.
        min: f64,
        /// The maximum allowed tempo.
        max: f64,
    },

    /// A sample rate Dhun does not support (SPEC §2: 44.1 kHz or 48 kHz).
    #[error("unsupported sample rate {0} Hz (supported: 44100, 48000)")]
    UnsupportedSampleRate(u32),

    /// A time signature Dhun does not support (SPEC §3.1: 4/4 only in v0.1).
    #[error("unsupported time signature {numerator}/{denominator} (v0.1 supports 4/4 only)")]
    UnsupportedTimeSignature {
        /// The rejected numerator (beats per bar).
        numerator: u8,
        /// The rejected denominator (beat unit).
        denominator: u8,
    },
}
