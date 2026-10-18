//! A simple feed-forward peak compressor (SPEC §3.5).

use crate::effects::Effect;
use crate::util::{db_to_lin, lin_to_db};

/// Compressor parameters.
#[derive(Debug, Clone, Copy)]
pub struct CompressorParams {
    /// Threshold in dBFS above which gain reduction begins.
    pub threshold_db: f32,
    /// Compression ratio (e.g. 4.0 = 4:1). Values ≥ 1.0.
    pub ratio: f32,
    /// Attack time in seconds.
    pub attack: f32,
    /// Release time in seconds.
    pub release: f32,
    /// Make-up gain in decibels applied to the output.
    pub makeup_db: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 4.0,
            attack: 0.005,
            release: 0.1,
            makeup_db: 0.0,
        }
    }
}

/// A feed-forward compressor with a peak-following detector.
#[derive(Debug, Clone)]
pub struct Compressor {
    params: CompressorParams,
    atk_coef: f32,
    rel_coef: f32,
    makeup: f32,
    env: f32,
}

impl Compressor {
    /// Creates a compressor at the given sample rate.
    #[must_use]
    pub fn new(sample_rate: f32, params: CompressorParams) -> Self {
        let mut c = Self {
            params,
            atk_coef: 0.0,
            rel_coef: 0.0,
            makeup: 1.0,
            env: 0.0,
        };
        c.update(sample_rate);
        c
    }

    fn update(&mut self, sample_rate: f32) {
        self.atk_coef = time_coef(self.params.attack, sample_rate);
        self.rel_coef = time_coef(self.params.release, sample_rate);
        self.makeup = db_to_lin(self.params.makeup_db);
    }

    /// Replaces the parameters.
    pub fn set_params(&mut self, sample_rate: f32, params: CompressorParams) {
        self.params = params;
        self.update(sample_rate);
    }
}

/// One-pole smoothing coefficient for a time constant in seconds.
fn time_coef(secs: f32, sample_rate: f32) -> f32 {
    if secs <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (secs * sample_rate)).exp()
    }
}

impl Effect for Compressor {
    fn tick(&mut self, input: f32) -> f32 {
        // Peak detector: fast attack, slow release toward |input|.
        let rectified = input.abs();
        let coef = if rectified > self.env {
            self.atk_coef
        } else {
            self.rel_coef
        };
        self.env += coef * (rectified - self.env);

        let env_db = lin_to_db(self.env);
        let gain = if env_db > self.params.threshold_db {
            let over = env_db - self.params.threshold_db;
            let reduction_db = over * (1.0 - 1.0 / self.params.ratio);
            db_to_lin(-reduction_db)
        } else {
            1.0
        };

        input * gain * self.makeup
    }

    fn reset(&mut self) {
        self.env = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oscillator::{Oscillator, Waveform};

    const SR: f32 = 48_000.0;

    /// Peak of a sine of the given amplitude through the compressor, settled.
    fn settled_peak(comp: &mut Compressor, amp: f32) -> f32 {
        let mut osc = Oscillator::new(SR, Waveform::Sine);
        osc.set_freq(220.0);
        for _ in 0..24_000 {
            comp.tick(osc.tick() * amp);
        }
        let mut peak = 0.0f32;
        for _ in 0..4800 {
            peak = peak.max(comp.tick(osc.tick() * amp).abs());
        }
        peak
    }

    #[test]
    fn below_threshold_passes_through() {
        let params = CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            makeup_db: 0.0,
            ..Default::default()
        };
        let mut comp = Compressor::new(SR, params);
        // Amplitude 0.05 ≈ -26 dBFS, below the -20 dB threshold.
        let peak = settled_peak(&mut comp, 0.05);
        assert!((peak - 0.05).abs() < 5e-3, "below-threshold peak {peak}");
    }

    #[test]
    fn above_threshold_is_attenuated() {
        let params = CompressorParams {
            threshold_db: -20.0,
            ratio: 4.0,
            makeup_db: 0.0,
            ..Default::default()
        };
        let mut comp = Compressor::new(SR, params);
        // Amplitude 0.5 ≈ -6 dBFS, well above the threshold.
        let peak = settled_peak(&mut comp, 0.5);
        assert!(peak < 0.5, "above-threshold output {peak} not reduced");
        assert!(peak > 0.0);
    }

    #[test]
    fn higher_input_yields_more_reduction() {
        let mut comp = Compressor::new(SR, CompressorParams::default());
        let quieter = settled_peak(&mut comp, 0.2);
        comp.reset();
        let louder = settled_peak(&mut comp, 0.8);
        // 4x the input should not produce 4x the output (it is compressed).
        assert!(
            louder / quieter < 4.0,
            "ratio not applied: {louder}/{quieter}"
        );
        assert!(louder.is_finite() && quieter.is_finite());
    }
}
