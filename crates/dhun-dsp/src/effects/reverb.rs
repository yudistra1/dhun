//! A mono Schroeder/Freeverb-style reverb (SPEC §3.5).
//!
//! Eight damped comb filters in parallel feed four all-pass filters in series.
//! The algorithm is the public-domain Freeverb design (Jezar), implemented from
//! its public description — no third-party code (CLAUDE.md clean-room rules).
//! Delay lines are allocated once at construction; `tick` never allocates.

use crate::effects::Effect;

/// Comb-filter delay lengths in samples at 44.1 kHz (Freeverb tuning).
const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// All-pass delay lengths in samples at 44.1 kHz (Freeverb tuning).
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];

const FIXED_GAIN: f32 = 0.015;
const SCALE_ROOM: f32 = 0.28;
const OFFSET_ROOM: f32 = 0.7;
const SCALE_DAMP: f32 = 0.4;

/// A damped feedback comb filter.
#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
    damp1: f32,
    damp2: f32,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            feedback: 0.5,
            damp1: 0.5,
            damp2: 0.5,
            store: 0.0,
        }
    }

    fn set(&mut self, feedback: f32, damp: f32) {
        self.feedback = feedback;
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.store = 0.0;
        self.idx = 0;
    }

    fn tick(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * self.damp2 + self.store * self.damp1;
        self.buf[self.idx] = input + self.store * self.feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// A Schroeder all-pass filter.
#[derive(Debug, Clone)]
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            feedback: 0.5,
        }
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.idx = 0;
    }

    fn tick(&mut self, input: f32) -> f32 {
        let buffered = self.buf[self.idx];
        let out = -input + buffered;
        self.buf[self.idx] = input + buffered * self.feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// Reverb parameters.
#[derive(Debug, Clone, Copy)]
pub struct ReverbParams {
    /// Room size in `[0.0, 1.0]` — larger is a longer tail.
    pub room_size: f32,
    /// High-frequency damping in `[0.0, 1.0]`.
    pub damping: f32,
    /// Wet (processed) level in `[0.0, 1.0]`.
    pub wet: f32,
    /// Dry (unprocessed) level in `[0.0, 1.0]`.
    pub dry: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 0.5,
            damping: 0.5,
            wet: 0.33,
            dry: 0.7,
        }
    }
}

/// A mono Freeverb-style reverb.
#[derive(Debug, Clone)]
pub struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    wet: f32,
    dry: f32,
}

impl Reverb {
    /// Creates a reverb, sizing delay lines for the sample rate.
    #[must_use]
    pub fn new(sample_rate: f32, params: ReverbParams) -> Self {
        let scale = sample_rate / 44_100.0;
        let combs = COMB_TUNING
            .iter()
            .map(|&len| Comb::new(scale_len(len, scale)))
            .collect();
        let allpasses = ALLPASS_TUNING
            .iter()
            .map(|&len| {
                let mut ap = Allpass::new(scale_len(len, scale));
                ap.feedback = 0.5;
                ap
            })
            .collect();
        let mut r = Self {
            combs,
            allpasses,
            wet: params.wet,
            dry: params.dry,
        };
        r.set_params(params);
        r
    }

    /// Updates the reverb parameters (does not clear the tail).
    pub fn set_params(&mut self, params: ReverbParams) {
        let feedback = params.room_size * SCALE_ROOM + OFFSET_ROOM;
        let damp = params.damping * SCALE_DAMP;
        for comb in &mut self.combs {
            comb.set(feedback, damp);
        }
        self.wet = params.wet;
        self.dry = params.dry;
    }
}

/// Scales a 44.1 kHz delay length to the target rate (at least 1 sample).
fn scale_len(len: usize, scale: f32) -> usize {
    ((len as f32 * scale) as usize).max(1)
}

impl Effect for Reverb {
    fn tick(&mut self, input: f32) -> f32 {
        let fed = input * FIXED_GAIN;
        let mut out = 0.0;
        for comb in &mut self.combs {
            out += comb.tick(fed);
        }
        for ap in &mut self.allpasses {
            out = ap.tick(out);
        }
        out * self.wet + input * self.dry
    }

    fn reset(&mut self) {
        self.combs.iter_mut().for_each(Comb::reset);
        self.allpasses.iter_mut().for_each(Allpass::reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Wet-only reverb so the tail is easy to measure.
    fn wet_reverb() -> Reverb {
        Reverb::new(
            SR,
            ReverbParams {
                room_size: 0.8,
                damping: 0.3,
                wet: 1.0,
                dry: 0.0,
            },
        )
    }

    #[test]
    fn impulse_produces_a_decaying_tail() {
        let mut rev = wet_reverb();
        let mut out = vec![0.0f32; SR as usize * 2];
        out[0] = rev.tick(1.0);
        for s in out.iter_mut().skip(1) {
            *s = rev.tick(0.0);
        }
        // There is meaningful energy spread beyond the first few ms.
        let tail_energy: f32 = out[4800..].iter().map(|s| s * s).sum();
        assert!(tail_energy > 1e-4, "reverb tail too weak: {tail_energy}");

        // ... and it has decayed to near-silence by the very end.
        let end_peak = out[out.len() - 2400..]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(end_peak < 1e-2, "reverb has not decayed: {end_peak}");

        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn dry_only_is_passthrough() {
        let mut rev = Reverb::new(
            SR,
            ReverbParams {
                wet: 0.0,
                dry: 1.0,
                ..Default::default()
            },
        );
        for x in [0.3, -0.7, 1.0, 0.0] {
            assert!((rev.tick(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn stays_finite_under_sustained_input() {
        let mut rev = wet_reverb();
        for i in 0..SR as usize {
            let x = if i % 2 == 0 { 0.5 } else { -0.5 };
            assert!(rev.tick(x).is_finite());
        }
    }
}
