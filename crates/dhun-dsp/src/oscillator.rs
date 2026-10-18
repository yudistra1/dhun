//! Band-unlimited (naive) waveform oscillators driven by a phase accumulator.
//!
//! The naive saw/square/triangle shapes alias at high frequencies; band-limiting
//! is deferred past v0.1. They are exact in phase and frequency, which is what
//! the DSP tests check.

use std::f32::consts::TAU;

/// The waveform an [`Oscillator`] produces (SPEC §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Waveform {
    /// Pure sine.
    #[default]
    Sine,
    /// Rising sawtooth.
    Saw,
    /// Square (50% duty).
    Square,
    /// Triangle.
    Triangle,
}

impl Waveform {
    /// Evaluates the waveform at a normalized phase in `[0.0, 1.0)`.
    #[must_use]
    pub fn eval(self, phase: f32) -> f32 {
        match self {
            Waveform::Sine => (phase * TAU).sin(),
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
        }
    }
}

/// A phase-accumulating oscillator. Allocation-free; advance one sample at a
/// time with [`Oscillator::tick`].
#[derive(Debug, Clone)]
pub struct Oscillator {
    sample_rate: f32,
    waveform: Waveform,
    phase: f32,
    freq: f32,
}

impl Oscillator {
    /// Creates an oscillator at the given sample rate.
    #[must_use]
    pub fn new(sample_rate: f32, waveform: Waveform) -> Self {
        Self {
            sample_rate,
            waveform,
            phase: 0.0,
            freq: 0.0,
        }
    }

    /// Sets the oscillator frequency in hertz.
    pub fn set_freq(&mut self, freq: f32) {
        self.freq = freq;
    }

    /// Sets the waveform.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Resets the phase to zero.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Advances one sample and returns the output in `[-1.0, 1.0]`.
    pub fn tick(&mut self) -> f32 {
        let out = self.waveform.eval(self.phase);
        self.phase += self.freq / self.sample_rate;
        // Wrap into [0, 1) without a loop; freq is assumed below sample_rate.
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        } else if self.phase < 0.0 {
            self.phase += 1.0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Counts rising zero-crossings (−→+) over one second of output.
    fn rising_zero_crossings(osc: &mut Oscillator) -> u32 {
        let mut prev = osc.tick();
        let mut count = 0;
        for _ in 0..(SR as usize) {
            let cur = osc.tick();
            if prev < 0.0 && cur >= 0.0 {
                count += 1;
            }
            prev = cur;
        }
        count
    }

    #[test]
    fn frequency_accuracy_all_waveforms() {
        for wf in [
            Waveform::Sine,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Triangle,
        ] {
            let mut osc = Oscillator::new(SR, wf);
            osc.set_freq(440.0);
            let crossings = rising_zero_crossings(&mut osc);
            // One rising crossing per cycle; expect ~440 over one second.
            assert!(
                (439..=441).contains(&crossings),
                "{wf:?}: {crossings} crossings, expected ~440"
            );
        }
    }

    #[test]
    fn output_bounded_and_finite() {
        for wf in [
            Waveform::Sine,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Triangle,
        ] {
            let mut osc = Oscillator::new(SR, wf);
            osc.set_freq(1000.0);
            for _ in 0..10_000 {
                let s = osc.tick();
                assert!(s.is_finite(), "{wf:?} produced non-finite sample");
                assert!((-1.0..=1.0).contains(&s), "{wf:?} out of range: {s}");
            }
        }
    }

    #[test]
    fn no_dc_offset_over_whole_periods() {
        // 480 Hz at 48 kHz => exactly 100 samples per cycle; average over whole
        // cycles should be ~0. Naive (non-band-limited) ramps pick up a tiny
        // sampling-artifact DC of ~1/samples_per_cycle; the tolerance below is
        // documented for that, and still catches any gross offset bug.
        const DC_TOL: f64 = 0.02;
        for wf in [
            Waveform::Sine,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Triangle,
        ] {
            let mut osc = Oscillator::new(SR, wf);
            osc.set_freq(480.0);
            let n = 48_000; // 480 whole cycles
            let mut sum = 0.0f64;
            for _ in 0..n {
                sum += f64::from(osc.tick());
            }
            let mean = sum / f64::from(n);
            assert!(mean.abs() < DC_TOL, "{wf:?} DC offset {mean}");
        }
    }
}
