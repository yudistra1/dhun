//! Time primitives: ticks (musical time), samples (audio time), the sample
//! rate, and the `PPQ` resolution constant.

use std::ops::{Add, Sub};

use crate::error::CoreError;

/// Pulses (ticks) per quarter note — Dhun's internal timing resolution
/// (SPEC §3.1: 960 PPQ).
pub const PPQ: u32 = 960;

/// A point or span in musical time, measured in ticks (`PPQ` ticks per quarter
/// note).
///
/// Signed so that relative offsets (e.g. latency compensation) can be
/// expressed; most values are non-negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ticks(i64);

impl Ticks {
    /// The zero point (start of the timeline).
    pub const ZERO: Ticks = Ticks(0);
    /// One quarter note, i.e. `PPQ` ticks.
    pub const QUARTER: Ticks = Ticks(PPQ as i64);

    /// Creates a tick value from a raw count.
    #[must_use]
    pub const fn new(ticks: i64) -> Self {
        Self(ticks)
    }

    /// Returns the raw tick count.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Add for Ticks {
    type Output = Ticks;
    fn add(self, rhs: Ticks) -> Ticks {
        Ticks(self.0 + rhs.0)
    }
}

impl Sub for Ticks {
    type Output = Ticks;
    fn sub(self, rhs: Ticks) -> Ticks {
        Ticks(self.0 - rhs.0)
    }
}

/// A point or span in audio time, measured in sample frames.
///
/// Signed for the same reason as [`Ticks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Samples(i64);

impl Samples {
    /// The zero point (start of the timeline).
    pub const ZERO: Samples = Samples(0);

    /// Creates a sample count from a raw value.
    #[must_use]
    pub const fn new(samples: i64) -> Self {
        Self(samples)
    }

    /// Returns the raw sample count.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Add for Samples {
    type Output = Samples;
    fn add(self, rhs: Samples) -> Samples {
        Samples(self.0 + rhs.0)
    }
}

impl Sub for Samples {
    type Output = Samples;
    fn sub(self, rhs: Samples) -> Samples {
        Samples(self.0 - rhs.0)
    }
}

/// A supported audio sample rate, in hertz (SPEC §2: 44.1 kHz or 48 kHz).
///
/// Construct via [`SampleRate::new`], which rejects unsupported rates, or use
/// the [`SampleRate::HZ_44100`] / [`SampleRate::HZ_48000`] constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// 44.1 kHz.
    pub const HZ_44100: SampleRate = SampleRate(44_100);
    /// 48 kHz.
    pub const HZ_48000: SampleRate = SampleRate(48_000);

    /// Creates a sample rate, rejecting any rate Dhun does not support.
    ///
    /// # Errors
    /// Returns [`CoreError::UnsupportedSampleRate`] for anything other than
    /// 44 100 or 48 000 Hz.
    pub fn new(hz: u32) -> Result<Self, CoreError> {
        match hz {
            44_100 | 48_000 => Ok(Self(hz)),
            other => Err(CoreError::UnsupportedSampleRate(other)),
        }
    }

    /// Returns the rate in hertz.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Returns the rate in hertz as an `f64`, for time conversions.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.0)
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::HZ_48000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppq_is_960() {
        assert_eq!(PPQ, 960);
        assert_eq!(Ticks::QUARTER.get(), 960);
    }

    #[test]
    fn ticks_arithmetic() {
        assert_eq!((Ticks::new(960) + Ticks::new(480)).get(), 1440);
        assert_eq!((Ticks::new(960) - Ticks::new(480)).get(), 480);
        assert_eq!(Ticks::ZERO.get(), 0);
    }

    #[test]
    fn samples_arithmetic() {
        assert_eq!((Samples::new(100) + Samples::new(50)).get(), 150);
        assert_eq!((Samples::new(100) - Samples::new(50)).get(), 50);
        assert_eq!(Samples::ZERO.get(), 0);
    }

    #[test]
    fn sample_rate_accepts_supported_rates() {
        assert_eq!(SampleRate::new(44_100).unwrap().get(), 44_100);
        assert_eq!(SampleRate::new(48_000).unwrap().get(), 48_000);
    }

    #[test]
    fn sample_rate_rejects_unsupported_rates() {
        assert_eq!(
            SampleRate::new(96_000),
            Err(CoreError::UnsupportedSampleRate(96_000))
        );
        assert_eq!(SampleRate::new(0), Err(CoreError::UnsupportedSampleRate(0)));
    }

    #[test]
    fn sample_rate_default_is_48k() {
        assert_eq!(SampleRate::default(), SampleRate::HZ_48000);
    }
}
