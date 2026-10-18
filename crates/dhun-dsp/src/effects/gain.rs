//! A simple gain stage (SPEC §3.5).

use crate::effects::Effect;
use crate::util::db_to_lin;

/// Multiplies the signal by a linear gain factor.
#[derive(Debug, Clone)]
pub struct Gain {
    gain: f32,
}

impl Gain {
    /// Unity gain (0 dB).
    #[must_use]
    pub fn unity() -> Self {
        Self { gain: 1.0 }
    }

    /// Creates a gain stage from a decibel value.
    #[must_use]
    pub fn from_db(db: f32) -> Self {
        Self {
            gain: db_to_lin(db),
        }
    }

    /// Creates a gain stage from a linear factor.
    #[must_use]
    pub fn from_linear(gain: f32) -> Self {
        Self { gain }
    }

    /// Sets the gain in decibels.
    pub fn set_db(&mut self, db: f32) {
        self.gain = db_to_lin(db);
    }

    /// Returns the current linear gain factor.
    #[must_use]
    pub fn linear(&self) -> f32 {
        self.gain
    }
}

impl Default for Gain {
    fn default() -> Self {
        Self::unity()
    }
}

impl Effect for Gain {
    fn tick(&mut self, input: f32) -> f32 {
        input * self.gain
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "expected {a} ≈ {b}");
    }

    #[test]
    fn unity_passes_through() {
        let mut g = Gain::unity();
        for x in [-1.0, -0.3, 0.0, 0.5, 1.0] {
            close(g.tick(x), x, 1e-9);
        }
    }

    #[test]
    fn plus_six_db_roughly_doubles() {
        let mut g = Gain::from_db(6.0206);
        close(g.tick(0.5), 1.0, 1e-3);
    }

    #[test]
    fn minus_six_db_roughly_halves() {
        let mut g = Gain::from_db(-6.0206);
        close(g.tick(1.0), 0.5, 1e-3);
    }

    #[test]
    fn block_process_matches_per_sample() {
        let mut g = Gain::from_linear(0.25);
        let mut buf = [1.0, 2.0, 4.0, -8.0];
        g.process(&mut buf);
        assert_eq!(buf, [0.25, 0.5, 1.0, -2.0]);
    }
}
