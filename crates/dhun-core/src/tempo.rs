//! Tempo, time signature, and the [`TempoMap`] that converts between musical
//! time (ticks) and audio time (samples / seconds).

use crate::error::CoreError;
use crate::time::{SampleRate, Samples, Ticks, PPQ};

/// A tempo in beats per minute, validated to Dhun's supported range
/// (SPEC §3.1: 40–240 BPM, default 120).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Bpm(f64);

impl Bpm {
    /// The slowest supported tempo.
    pub const MIN: f64 = 40.0;
    /// The fastest supported tempo.
    pub const MAX: f64 = 240.0;
    /// The default tempo.
    pub const DEFAULT: f64 = 120.0;

    /// Creates a tempo, rejecting non-finite or out-of-range values.
    ///
    /// # Errors
    /// Returns [`CoreError::TempoOutOfRange`] when `bpm` is not finite or lies
    /// outside `MIN..=MAX`.
    pub fn new(bpm: f64) -> Result<Self, CoreError> {
        if bpm.is_finite() && (Self::MIN..=Self::MAX).contains(&bpm) {
            Ok(Self(bpm))
        } else {
            Err(CoreError::TempoOutOfRange {
                value: bpm,
                min: Self::MIN,
                max: Self::MAX,
            })
        }
    }

    /// Returns the tempo in beats per minute.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Default for Bpm {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

/// A time signature. v0.1 supports 4/4 only (SPEC §3.1), but the type exists so
/// higher layers and the project format are already signature-aware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeSignature {
    numerator: u8,
    denominator: u8,
}

impl TimeSignature {
    /// Common time, 4/4 — the only signature supported in v0.1.
    pub const FOUR_FOUR: TimeSignature = TimeSignature {
        numerator: 4,
        denominator: 4,
    };

    /// Creates a time signature, rejecting anything unsupported in v0.1.
    ///
    /// # Errors
    /// Returns [`CoreError::UnsupportedTimeSignature`] for any signature other
    /// than 4/4.
    pub fn new(numerator: u8, denominator: u8) -> Result<Self, CoreError> {
        if numerator == 4 && denominator == 4 {
            Ok(Self::FOUR_FOUR)
        } else {
            Err(CoreError::UnsupportedTimeSignature {
                numerator,
                denominator,
            })
        }
    }

    /// Beats per bar.
    #[must_use]
    pub const fn numerator(self) -> u8 {
        self.numerator
    }

    /// The note value that gets one beat (e.g. 4 = quarter note).
    #[must_use]
    pub const fn denominator(self) -> u8 {
        self.denominator
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::FOUR_FOUR
    }
}

/// Maps between musical time (ticks) and audio time (samples / seconds).
///
/// v0.1 models a single constant tempo in 4/4 (SPEC §3.1). All conversions are
/// exposed as methods so a piecewise tempo map can replace the implementation
/// later without changing call sites.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMap {
    tempo: Bpm,
    sample_rate: SampleRate,
    time_signature: TimeSignature,
}

impl TempoMap {
    /// Creates a constant-tempo map in 4/4.
    #[must_use]
    pub fn new(tempo: Bpm, sample_rate: SampleRate) -> Self {
        Self {
            tempo,
            sample_rate,
            time_signature: TimeSignature::FOUR_FOUR,
        }
    }

    /// The tempo.
    #[must_use]
    pub fn tempo(self) -> Bpm {
        self.tempo
    }

    /// The sample rate.
    #[must_use]
    pub fn sample_rate(self) -> SampleRate {
        self.sample_rate
    }

    /// The time signature.
    #[must_use]
    pub fn time_signature(self) -> TimeSignature {
        self.time_signature
    }

    /// Seconds per quarter note at this tempo.
    #[must_use]
    pub fn seconds_per_quarter(self) -> f64 {
        60.0 / self.tempo.get()
    }

    /// Seconds per tick.
    #[must_use]
    pub fn seconds_per_tick(self) -> f64 {
        self.seconds_per_quarter() / f64::from(PPQ)
    }

    /// Audio sample frames per tick. May be fractional (e.g. at 44.1 kHz).
    #[must_use]
    pub fn samples_per_tick(self) -> f64 {
        self.seconds_per_tick() * self.sample_rate.as_f64()
    }

    /// Converts ticks to seconds.
    #[must_use]
    pub fn ticks_to_seconds(self, ticks: Ticks) -> f64 {
        ticks.get() as f64 * self.seconds_per_tick()
    }

    /// Converts seconds to the nearest whole tick.
    #[must_use]
    pub fn seconds_to_ticks(self, seconds: f64) -> Ticks {
        Ticks::new((seconds / self.seconds_per_tick()).round() as i64)
    }

    /// Converts ticks to the nearest whole sample frame.
    #[must_use]
    pub fn ticks_to_samples(self, ticks: Ticks) -> Samples {
        Samples::new((ticks.get() as f64 * self.samples_per_tick()).round() as i64)
    }

    /// Converts sample frames to the nearest whole tick.
    #[must_use]
    pub fn samples_to_ticks(self, samples: Samples) -> Ticks {
        Ticks::new((samples.get() as f64 / self.samples_per_tick()).round() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts two `f64`s are equal within a tight absolute tolerance.
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {a} ≈ {b}");
    }

    #[test]
    fn bpm_default_is_120() {
        assert_eq!(Bpm::default().get(), 120.0);
    }

    #[test]
    fn bpm_accepts_range_bounds() {
        assert_eq!(Bpm::new(40.0).unwrap().get(), 40.0);
        assert_eq!(Bpm::new(120.0).unwrap().get(), 120.0);
        assert_eq!(Bpm::new(240.0).unwrap().get(), 240.0);
    }

    #[test]
    fn bpm_rejects_out_of_range_and_non_finite() {
        assert!(Bpm::new(39.9).is_err());
        assert!(Bpm::new(240.1).is_err());
        assert!(Bpm::new(0.0).is_err());
        assert!(Bpm::new(f64::NAN).is_err());
        assert!(Bpm::new(f64::INFINITY).is_err());
    }

    #[test]
    fn time_signature_only_allows_four_four() {
        assert_eq!(TimeSignature::new(4, 4).unwrap(), TimeSignature::FOUR_FOUR);
        assert_eq!(TimeSignature::default(), TimeSignature::FOUR_FOUR);
        assert!(TimeSignature::new(3, 4).is_err());
        assert!(TimeSignature::new(7, 8).is_err());
    }

    #[test]
    fn conversions_120bpm_48k_are_exact() {
        // 120 BPM => quarter note = 0.5 s. At 48 kHz that is 24 000 samples,
        // and 24 000 / 960 PPQ = 25 samples per tick exactly.
        let map = TempoMap::new(Bpm::new(120.0).unwrap(), SampleRate::HZ_48000);
        close(map.seconds_per_quarter(), 0.5);
        close(map.samples_per_tick(), 25.0);
        assert_eq!(map.ticks_to_samples(Ticks::QUARTER), Samples::new(24_000));
        assert_eq!(map.ticks_to_samples(Ticks::new(1)), Samples::new(25));
        close(map.ticks_to_seconds(Ticks::QUARTER), 0.5);
    }

    #[test]
    fn conversions_roundtrip() {
        let map = TempoMap::new(Bpm::new(120.0).unwrap(), SampleRate::HZ_48000);
        assert_eq!(map.samples_to_ticks(Samples::new(24_000)), Ticks::QUARTER);
        assert_eq!(map.seconds_to_ticks(0.5), Ticks::QUARTER);
        let t = Ticks::new(7_680);
        assert_eq!(map.samples_to_ticks(map.ticks_to_samples(t)), t);
    }

    #[test]
    fn conversions_60bpm_one_second_quarter() {
        let map = TempoMap::new(Bpm::new(60.0).unwrap(), SampleRate::HZ_48000);
        close(map.ticks_to_seconds(Ticks::QUARTER), 1.0);
        assert_eq!(map.ticks_to_samples(Ticks::QUARTER), Samples::new(48_000));
    }

    #[test]
    fn conversions_44100_fractional_samples_per_tick() {
        // 120 BPM quarter = 0.5 s = 22 050 samples at 44.1 kHz; 22 050 / 960 is
        // fractional, so samples_per_tick is not an integer but the quarter note
        // still lands on a whole sample count.
        let map = TempoMap::new(Bpm::new(120.0).unwrap(), SampleRate::HZ_44100);
        close(map.samples_per_tick(), 22_050.0 / 960.0);
        assert_eq!(map.ticks_to_samples(Ticks::QUARTER), Samples::new(22_050));
    }
}
