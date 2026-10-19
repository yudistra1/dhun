//! UI-side meter state: smooths the engine's raw block peaks with a falloff and
//! latches a clip indicator, for drawing level meters (SPEC §3.5).

/// Per-frame falloff applied to held peaks (~0.3 s to fall at 60 fps).
const DECAY: f32 = 0.85;
/// Level at/above which a channel is considered clipping.
const CLIP: f32 = 0.999;
/// Below this, a meter is treated as idle.
const SILENCE: f32 = 1e-3;

/// Smoothed level meters for the mixer channels and master.
#[derive(Debug, Default)]
pub struct Meters {
    channels: Vec<f32>,
    clips: Vec<bool>,
    master_l: f32,
    master_r: f32,
    master_clip: bool,
}

impl Meters {
    fn ensure(&mut self, n: usize) {
        if self.channels.len() < n {
            self.channels.resize(n, 0.0);
            self.clips.resize(n, false);
        }
    }

    /// Feeds the latest raw peaks (per channel, and master L/R). Applies
    /// falloff so meters fall smoothly, and latches clip flags. Passing fewer
    /// peaks than known channels decays the rest toward zero.
    pub fn feed(&mut self, channel_peaks: &[f32], master: (f32, f32)) {
        self.ensure(channel_peaks.len());
        for i in 0..self.channels.len() {
            let p = channel_peaks.get(i).copied().unwrap_or(0.0);
            self.channels[i] = p.max(self.channels[i] * DECAY);
            if p >= CLIP {
                self.clips[i] = true;
            }
        }
        self.master_l = master.0.max(self.master_l * DECAY);
        self.master_r = master.1.max(self.master_r * DECAY);
        if master.0 >= CLIP || master.1 >= CLIP {
            self.master_clip = true;
        }
    }

    /// Clears latched clip indicators (e.g. on transport start).
    pub fn reset_clips(&mut self) {
        self.clips.iter_mut().for_each(|c| *c = false);
        self.master_clip = false;
    }

    /// Level and clip flag for channel `i`.
    #[must_use]
    pub fn channel(&self, i: usize) -> (f32, bool) {
        (
            self.channels.get(i).copied().unwrap_or(0.0),
            self.clips.get(i).copied().unwrap_or(false),
        )
    }

    /// Master `(left, right)` levels and clip flag.
    #[must_use]
    pub fn master(&self) -> (f32, f32, bool) {
        (self.master_l, self.master_r, self.master_clip)
    }

    /// Whether any meter is still above silence (used to keep repainting the
    /// falloff after the transport stops).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.master_l > SILENCE
            || self.master_r > SILENCE
            || self.channels.iter().any(|&c| c > SILENCE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_hold_then_decay() {
        let mut m = Meters::default();
        m.feed(&[1.0, 0.0], (1.0, 0.5));
        let (l0, clipped) = m.channel(0);
        assert!((l0 - 1.0).abs() < 1e-6);
        assert!(clipped, "1.0 should latch a clip");
        // Next frame with no signal decays, not drops to zero.
        m.feed(&[0.0, 0.0], (0.0, 0.0));
        let (l1, _) = m.channel(0);
        assert!(l1 < 1.0 && l1 > 0.0, "should decay: {l1}");
        // Clip stays latched until reset.
        assert!(m.channel(0).1);
        m.reset_clips();
        assert!(!m.channel(0).1);
    }

    #[test]
    fn is_active_tracks_decay_to_silence() {
        let mut m = Meters::default();
        m.feed(&[0.5], (0.5, 0.5));
        assert!(m.is_active());
        for _ in 0..200 {
            m.feed(&[0.0], (0.0, 0.0));
        }
        assert!(!m.is_active(), "should settle to silence");
    }
}
