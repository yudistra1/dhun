//! A 3-band EQ (low shelf, mid peak, high shelf) built from RBJ biquads
//! (SPEC §3.5).

use std::f32::consts::PI;

use crate::effects::Effect;

/// A Direct-Form-I biquad. Coefficients are normalized by `a0` at construction.
#[derive(Debug, Clone)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn from_coeffs(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ low-shelf (shelf slope S = 1).
    fn low_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha),
            (a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha,
        )
    }

    /// RBJ high-shelf (shelf slope S = 1).
    fn high_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * std::f32::consts::SQRT_2;
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        Self::from_coeffs(
            a * ((a + 1.0) + (a - 1.0) * cos + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - two_sqrt_a_alpha),
            (a + 1.0) - (a - 1.0) * cos + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - two_sqrt_a_alpha,
        )
    }

    /// RBJ peaking EQ.
    fn peaking(sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from_coeffs(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Band gains for a [`ThreeBandEq`]. Corner/center frequencies are fixed in
/// v0.1; only the gains are adjustable.
#[derive(Debug, Clone, Copy)]
pub struct ThreeBandEqParams {
    /// Low-shelf gain in decibels.
    pub low_db: f32,
    /// Mid peak gain in decibels.
    pub mid_db: f32,
    /// High-shelf gain in decibels.
    pub high_db: f32,
}

impl Default for ThreeBandEqParams {
    fn default() -> Self {
        Self {
            low_db: 0.0,
            mid_db: 0.0,
            high_db: 0.0,
        }
    }
}

/// A 3-band equalizer: low shelf @ 250 Hz, mid peak @ 1 kHz (Q 0.9), high shelf
/// @ 4 kHz, applied in series.
#[derive(Debug, Clone)]
pub struct ThreeBandEq {
    sample_rate: f32,
    low: Biquad,
    mid: Biquad,
    high: Biquad,
}

impl ThreeBandEq {
    const LOW_FREQ: f32 = 250.0;
    const MID_FREQ: f32 = 1_000.0;
    const MID_Q: f32 = 0.9;
    const HIGH_FREQ: f32 = 4_000.0;

    /// Creates an EQ with the given band gains.
    #[must_use]
    pub fn new(sample_rate: f32, params: ThreeBandEqParams) -> Self {
        Self {
            sample_rate,
            low: Biquad::low_shelf(sample_rate, Self::LOW_FREQ, params.low_db),
            mid: Biquad::peaking(sample_rate, Self::MID_FREQ, Self::MID_Q, params.mid_db),
            high: Biquad::high_shelf(sample_rate, Self::HIGH_FREQ, params.high_db),
        }
    }

    /// Updates the band gains, preserving filter state.
    pub fn set_params(&mut self, params: ThreeBandEqParams) {
        let (x1, x2, y1, y2) = (self.low.x1, self.low.x2, self.low.y1, self.low.y2);
        self.low = Biquad::low_shelf(self.sample_rate, Self::LOW_FREQ, params.low_db);
        (self.low.x1, self.low.x2, self.low.y1, self.low.y2) = (x1, x2, y1, y2);

        let (x1, x2, y1, y2) = (self.mid.x1, self.mid.x2, self.mid.y1, self.mid.y2);
        self.mid = Biquad::peaking(self.sample_rate, Self::MID_FREQ, Self::MID_Q, params.mid_db);
        (self.mid.x1, self.mid.x2, self.mid.y1, self.mid.y2) = (x1, x2, y1, y2);

        let (x1, x2, y1, y2) = (self.high.x1, self.high.x2, self.high.y1, self.high.y2);
        self.high = Biquad::high_shelf(self.sample_rate, Self::HIGH_FREQ, params.high_db);
        (self.high.x1, self.high.x2, self.high.y1, self.high.y2) = (x1, x2, y1, y2);
    }
}

impl Effect for ThreeBandEq {
    fn tick(&mut self, input: f32) -> f32 {
        self.high.tick(self.mid.tick(self.low.tick(input)))
    }

    fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oscillator::{Oscillator, Waveform};
    use crate::util::db_to_lin;

    const SR: f32 = 48_000.0;

    /// Peak amplitude of a sine at `freq` through `eq` after settling.
    fn settled_peak(eq: &mut ThreeBandEq, freq: f32) -> f32 {
        let mut osc = Oscillator::new(SR, Waveform::Sine);
        osc.set_freq(freq);
        for _ in 0..4800 {
            eq.tick(osc.tick());
        }
        let mut peak = 0.0f32;
        for _ in 0..4800 {
            peak = peak.max(eq.tick(osc.tick()).abs());
        }
        peak
    }

    #[test]
    fn flat_eq_passes_through() {
        let mut eq = ThreeBandEq::new(SR, ThreeBandEqParams::default());
        let mut osc = Oscillator::new(SR, Waveform::Sine);
        osc.set_freq(1000.0);
        let mut max_err = 0.0f32;
        for _ in 0..4800 {
            let x = osc.tick();
            max_err = max_err.max((eq.tick(x) - x).abs());
        }
        assert!(max_err < 1e-3, "flat EQ altered signal by {max_err}");
    }

    #[test]
    fn low_shelf_boost_raises_dc_gain() {
        let params = ThreeBandEqParams {
            low_db: 12.0,
            ..Default::default()
        };
        let mut eq = ThreeBandEq::new(SR, params);
        let mut y = 0.0;
        for _ in 0..20_000 {
            y = eq.tick(1.0); // DC
        }
        // At DC the low shelf applies its full gain; mid/high shelves are unity.
        assert!(
            (y - db_to_lin(12.0)).abs() < 0.05,
            "DC gain {y}, expected ~{}",
            db_to_lin(12.0)
        );
    }

    #[test]
    fn high_shelf_boost_lifts_highs_more_than_lows() {
        let params = ThreeBandEqParams {
            high_db: 12.0,
            ..Default::default()
        };
        let mut eq = ThreeBandEq::new(SR, params);
        let low = settled_peak(&mut eq, 100.0);
        eq.reset();
        let high = settled_peak(&mut eq, 12_000.0);
        assert!(low < 1.1, "low band should be ~unity, got {low}");
        assert!(high > 2.0, "high band should be boosted, got {high}");
    }

    #[test]
    fn output_is_finite() {
        let params = ThreeBandEqParams {
            low_db: 9.0,
            mid_db: -6.0,
            high_db: 9.0,
        };
        let mut eq = ThreeBandEq::new(SR, params);
        let mut osc = Oscillator::new(SR, Waveform::Saw);
        osc.set_freq(220.0);
        for _ in 0..48_000 {
            assert!(eq.tick(osc.tick()).is_finite());
        }
    }
}
