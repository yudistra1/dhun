//! A zero-delay-feedback (TPT) state-variable low-pass filter with cutoff and
//! resonance controls (SPEC §3.3).
//!
//! Topology-preserving transform SVF after Andrew Simper's public derivation —
//! numerically stable and well-defined at audio rates. Allocation-free.

use std::f32::consts::PI;

/// A resonant low-pass filter. Set `cutoff`/`resonance`, then [`SvfLowpass::tick`]
/// one sample at a time.
#[derive(Debug, Clone)]
pub struct SvfLowpass {
    sample_rate: f32,
    cutoff: f32,
    q: f32,
    // Precomputed coefficients.
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    // State.
    ic1eq: f32,
    ic2eq: f32,
}

impl SvfLowpass {
    /// Creates a low-pass filter. `q` is the resonance (0.5 ≈ no resonance;
    /// higher values peak at the cutoff).
    #[must_use]
    pub fn new(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        let mut f = Self {
            sample_rate,
            cutoff,
            q,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            k: 0.0,
            ic1eq: 0.0,
            ic2eq: 0.0,
        };
        f.update();
        f
    }

    /// Sets the cutoff frequency in hertz (clamped below Nyquist).
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff;
        self.update();
    }

    /// Sets the resonance (Q). Clamped to a safe minimum to avoid division by
    /// zero.
    pub fn set_resonance(&mut self, q: f32) {
        self.q = q;
        self.update();
    }

    /// Clears the internal state.
    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    fn update(&mut self) {
        let nyquist = self.sample_rate * 0.5;
        let cutoff = self.cutoff.clamp(1.0, nyquist - 1.0);
        let q = self.q.max(0.025);
        let g = (PI * cutoff / self.sample_rate).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Processes one input sample and returns the low-pass output.
    pub fn tick(&mut self, input: f32) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oscillator::{Oscillator, Waveform};

    const SR: f32 = 48_000.0;

    /// Peak absolute amplitude of a sine at `freq` after the filter settles.
    fn settled_peak(freq: f32, cutoff: f32) -> f32 {
        let mut osc = Oscillator::new(SR, Waveform::Sine);
        osc.set_freq(freq);
        let mut filt = SvfLowpass::new(SR, cutoff, 0.707);
        // Settle for 20 ms, then measure peak over 20 ms.
        for _ in 0..960 {
            filt.tick(osc.tick());
        }
        let mut peak = 0.0f32;
        for _ in 0..960 {
            let y = filt.tick(osc.tick());
            peak = peak.max(y.abs());
        }
        peak
    }

    #[test]
    fn passes_dc() {
        let mut filt = SvfLowpass::new(SR, 1000.0, 0.707);
        let mut y = 0.0;
        for _ in 0..5000 {
            y = filt.tick(1.0);
        }
        assert!((y - 1.0).abs() < 1e-3, "DC steady-state {y}");
    }

    #[test]
    fn passes_frequencies_below_cutoff() {
        let peak = settled_peak(100.0, 1000.0);
        assert!(
            peak > 0.9,
            "100 Hz through 1 kHz LP should pass, peak {peak}"
        );
    }

    #[test]
    fn attenuates_frequencies_above_cutoff() {
        let peak = settled_peak(10_000.0, 1000.0);
        assert!(
            peak < 0.1,
            "10 kHz through 1 kHz LP should be attenuated, peak {peak}"
        );
    }

    #[test]
    fn roughly_minus_3db_at_cutoff() {
        let peak = settled_peak(1000.0, 1000.0);
        // -3 dB ≈ 0.707; allow a generous window.
        assert!((0.6..0.85).contains(&peak), "gain at cutoff {peak}");
    }

    #[test]
    fn output_is_finite() {
        let mut osc = Oscillator::new(SR, Waveform::Saw);
        osc.set_freq(220.0);
        let mut filt = SvfLowpass::new(SR, 800.0, 4.0); // high resonance
        for _ in 0..48_000 {
            let y = filt.tick(osc.tick());
            assert!(y.is_finite());
        }
    }
}
