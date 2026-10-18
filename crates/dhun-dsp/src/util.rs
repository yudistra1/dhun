//! Small, allocation-free DSP helpers: pitch/gain conversions and a
//! deterministic noise source.

/// Converts a MIDI note number to its frequency in hertz (A4 = note 69 = 440 Hz).
///
/// Accepts a fractional note so that detune/pitch-bend can be expressed.
#[must_use]
pub fn note_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

/// Converts a gain in decibels to a linear amplitude factor.
#[must_use]
pub fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear amplitude factor to decibels.
///
/// Non-positive inputs return [`f32::NEG_INFINITY`] (silence).
#[must_use]
pub fn lin_to_db(lin: f32) -> f32 {
    if lin > 0.0 {
        20.0 * lin.log10()
    } else {
        f32::NEG_INFINITY
    }
}

/// A tiny, deterministic pseudo-random noise source (xorshift32).
///
/// Deterministic for a given seed so that offline renders are bit-identical
/// across runs (SPEC §5). Not cryptographically secure — it only needs to sound
/// like noise.
#[derive(Debug, Clone)]
pub struct NoiseRng {
    state: u32,
}

impl NoiseRng {
    /// Creates a noise source from a seed. A zero seed is remapped so the
    /// generator never gets stuck at zero.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x1 } else { seed },
        }
    }

    /// Returns the next raw 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Returns the next noise sample in the range `[-1.0, 1.0)`.
    pub fn next_bipolar(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "expected {a} ≈ {b} (eps {eps})");
    }

    #[test]
    fn note_to_freq_known_pitches() {
        close(note_to_freq(69.0), 440.0, 1e-3); // A4
        close(note_to_freq(57.0), 220.0, 1e-3); // A3
        close(note_to_freq(81.0), 880.0, 1e-3); // A5
        close(note_to_freq(60.0), 261.625_6, 1e-2); // middle C
    }

    #[test]
    fn db_lin_roundtrip() {
        close(db_to_lin(0.0), 1.0, 1e-6);
        close(db_to_lin(6.0206), 2.0, 1e-3);
        close(db_to_lin(-6.0206), 0.5, 1e-3);
        for db in [-24.0, -6.0, 0.0, 3.0, 12.0] {
            close(lin_to_db(db_to_lin(db)), db, 1e-3);
        }
        assert_eq!(lin_to_db(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let mut a = NoiseRng::new(12345);
        let mut b = NoiseRng::new(12345);
        for _ in 0..1000 {
            let x = a.next_bipolar();
            assert_eq!(x, b.next_bipolar());
            assert!((-1.0..1.0).contains(&x));
            assert!(x.is_finite());
        }
    }

    #[test]
    fn noise_zero_seed_does_not_stick() {
        let mut r = NoiseRng::new(0);
        assert_ne!(r.next_u32(), 0);
        assert_ne!(r.next_u32(), 0);
    }
}
