//! A linear ADSR amplitude envelope (SPEC §3.3).
//!
//! Linear segments are used (not exponential) so stage timing is exact and
//! testable. Times are in seconds; the sustain level is a linear amplitude in
//! `[0.0, 1.0]`.

/// The stage an [`Adsr`] envelope is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Silent and inactive.
    Idle,
    /// Ramping up to 1.0.
    Attack,
    /// Falling from 1.0 to the sustain level.
    Decay,
    /// Holding the sustain level until note-off.
    Sustain,
    /// Falling from the current level to 0.0.
    Release,
}

/// ADSR parameters. Times are in seconds; `sustain` is a linear level.
#[derive(Debug, Clone, Copy)]
pub struct AdsrParams {
    /// Attack time in seconds.
    pub attack: f32,
    /// Decay time in seconds.
    pub decay: f32,
    /// Sustain level in `[0.0, 1.0]`.
    pub sustain: f32,
    /// Release time in seconds.
    pub release: f32,
}

impl Default for AdsrParams {
    fn default() -> Self {
        Self {
            attack: 0.005,
            decay: 0.1,
            sustain: 0.7,
            release: 0.2,
        }
    }
}

/// A linear ADSR envelope generator. Allocation-free.
#[derive(Debug, Clone)]
pub struct Adsr {
    sample_rate: f32,
    params: AdsrParams,
    stage: Stage,
    level: f32,
}

impl Adsr {
    /// Creates an idle envelope.
    #[must_use]
    pub fn new(sample_rate: f32, params: AdsrParams) -> Self {
        Self {
            sample_rate,
            params,
            stage: Stage::Idle,
            level: 0.0,
        }
    }

    /// Updates the envelope parameters without resetting the current level.
    pub fn set_params(&mut self, params: AdsrParams) {
        self.params = params;
    }

    /// The current stage.
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Whether the envelope is producing (or about to produce) sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Starts (or retriggers) the envelope from the attack stage.
    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    /// Begins the release stage from the current level.
    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Per-sample increment to traverse a full `0.0..=1.0` span in `secs`.
    fn rate(&self, secs: f32) -> f32 {
        if secs <= 0.0 {
            1.0
        } else {
            1.0 / (secs * self.sample_rate)
        }
    }

    /// Advances one sample and returns the current amplitude in `[0.0, 1.0]`.
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => self.level = 0.0,
            Stage::Attack => {
                self.level += self.rate(self.params.attack);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= self.rate(self.params.decay) * (1.0 - self.params.sustain);
                if self.level <= self.params.sustain {
                    self.level = self.params.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = self.params.sustain,
            Stage::Release => {
                self.level -= self.rate(self.params.release);
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn params() -> AdsrParams {
        AdsrParams {
            attack: 0.01, // 480 samples
            decay: 0.01,  // 480 samples
            sustain: 0.5,
            release: 0.01, // 480 samples
        }
    }

    #[test]
    fn starts_idle_and_silent() {
        let mut env = Adsr::new(SR, params());
        assert_eq!(env.stage(), Stage::Idle);
        assert!(!env.is_active());
        assert_eq!(env.tick(), 0.0);
    }

    #[test]
    fn attack_reaches_full_after_attack_time() {
        let mut env = Adsr::new(SR, params());
        env.note_on();
        // 0.01 s * 48 kHz = 480 samples to climb from 0 to 1.
        let mut last = 0.0;
        for _ in 0..480 {
            last = env.tick();
        }
        assert!((last - 1.0).abs() < 1e-3, "attack peak {last}");
        assert_eq!(env.stage(), Stage::Decay);
    }

    #[test]
    fn decays_to_sustain_and_holds() {
        let mut env = Adsr::new(SR, params());
        env.note_on();
        for _ in 0..(480 + 480 + 100) {
            env.tick();
        }
        assert_eq!(env.stage(), Stage::Sustain);
        let held = env.tick();
        assert!((held - 0.5).abs() < 1e-3, "sustain level {held}");
    }

    #[test]
    fn release_falls_to_silence_and_goes_idle() {
        let mut env = Adsr::new(SR, params());
        env.note_on();
        for _ in 0..2000 {
            env.tick();
        }
        env.note_off();
        assert_eq!(env.stage(), Stage::Release);
        for _ in 0..480 {
            env.tick();
        }
        let v = env.tick();
        assert!(v.abs() < 1e-3, "post-release level {v}");
        assert_eq!(env.stage(), Stage::Idle);
        assert!(!env.is_active());
    }

    #[test]
    fn stays_finite_and_bounded() {
        let mut env = Adsr::new(SR, params());
        env.note_on();
        for i in 0..5000 {
            if i == 2000 {
                env.note_off();
            }
            let v = env.tick();
            assert!(v.is_finite());
            assert!((0.0..=1.0).contains(&v), "level out of range: {v}");
        }
    }
}
