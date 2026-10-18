//! A built-in subtractive synthesizer: two oscillators (with detune) into a
//! resonant low-pass filter and an ADSR amp envelope, 8-voice polyphonic
//! (SPEC §3.3).
//!
//! All voices are pre-allocated at construction; `note_on`/`note_off`/`tick`
//! never allocate.

use crate::envelope::{Adsr, AdsrParams};
use crate::filter::SvfLowpass;
use crate::oscillator::{Oscillator, Waveform};
use crate::util::note_to_freq;

/// Number of simultaneous voices (SPEC §3.3: 8-voice polyphony).
pub const VOICES: usize = 8;

/// Patch parameters shared by all voices.
#[derive(Debug, Clone, Copy)]
pub struct SynthParams {
    /// Waveform of oscillator 1.
    pub osc1: Waveform,
    /// Waveform of oscillator 2.
    pub osc2: Waveform,
    /// Detune of oscillator 2 relative to oscillator 1, in cents.
    pub detune_cents: f32,
    /// Oscillator balance: 0.0 = osc1 only, 1.0 = osc2 only.
    pub mix: f32,
    /// Low-pass cutoff in hertz.
    pub cutoff: f32,
    /// Low-pass resonance (Q).
    pub resonance: f32,
    /// Amp envelope.
    pub amp: AdsrParams,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            osc1: Waveform::Saw,
            osc2: Waveform::Saw,
            detune_cents: 7.0,
            mix: 0.5,
            cutoff: 8_000.0,
            resonance: 0.707,
            amp: AdsrParams::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct Voice {
    osc1: Oscillator,
    osc2: Oscillator,
    filter: SvfLowpass,
    amp: Adsr,
    note: u8,
    velocity: f32,
    mix: f32,
    age: u64,
}

impl Voice {
    fn new(sample_rate: f32, params: &SynthParams) -> Self {
        Self {
            osc1: Oscillator::new(sample_rate, params.osc1),
            osc2: Oscillator::new(sample_rate, params.osc2),
            filter: SvfLowpass::new(sample_rate, params.cutoff, params.resonance),
            amp: Adsr::new(sample_rate, params.amp),
            note: 0,
            velocity: 0.0,
            mix: params.mix,
            age: 0,
        }
    }

    fn trigger(&mut self, note: u8, velocity: f32, params: &SynthParams, age: u64) {
        let f1 = note_to_freq(f32::from(note));
        let f2 = note_to_freq(f32::from(note) + params.detune_cents / 100.0);
        self.osc1.set_waveform(params.osc1);
        self.osc2.set_waveform(params.osc2);
        self.osc1.set_freq(f1);
        self.osc2.set_freq(f2);
        self.osc1.reset();
        self.osc2.reset();
        self.filter.set_cutoff(params.cutoff);
        self.filter.set_resonance(params.resonance);
        self.filter.reset();
        self.amp.set_params(params.amp);
        self.amp.note_on();
        self.note = note;
        self.velocity = velocity;
        self.mix = params.mix;
        self.age = age;
    }

    fn is_active(&self) -> bool {
        self.amp.is_active()
    }

    fn tick(&mut self) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        let o = self.osc1.tick() * (1.0 - self.mix) + self.osc2.tick() * self.mix;
        let filtered = self.filter.tick(o);
        filtered * self.amp.tick() * self.velocity
    }
}

/// An 8-voice subtractive synth.
#[derive(Debug, Clone)]
pub struct Synth {
    params: SynthParams,
    voices: Vec<Voice>,
    next_age: u64,
}

impl Synth {
    /// Creates a synth with all voices pre-allocated.
    #[must_use]
    pub fn new(sample_rate: f32, params: SynthParams) -> Self {
        let voices = (0..VOICES)
            .map(|_| Voice::new(sample_rate, &params))
            .collect();
        Self {
            params,
            voices,
            next_age: 0,
        }
    }

    /// Replaces the patch parameters, applied to subsequently triggered notes.
    pub fn set_params(&mut self, params: SynthParams) {
        self.params = params;
    }

    /// Number of voices currently producing (or releasing) sound.
    #[must_use]
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    /// Starts a note. `velocity` is a linear gain in `[0.0, 1.0]`. If all voices
    /// are busy, the oldest is stolen.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let age = self.next_age;
        self.next_age += 1;
        let params = self.params;
        let idx = self.pick_voice();
        self.voices[idx].trigger(note, velocity, &params, age);
    }

    /// Releases the most recent voice playing `note` (enters its release stage).
    pub fn note_off(&mut self, note: u8) {
        if let Some(v) = self
            .voices
            .iter_mut()
            .filter(|v| v.is_active() && v.note == note)
            .max_by_key(|v| v.age)
        {
            v.amp.note_off();
        }
    }

    /// Advances one sample, summing all voices.
    pub fn tick(&mut self) -> f32 {
        self.voices.iter_mut().map(Voice::tick).sum()
    }

    /// Chooses a free voice, or the oldest active one to steal.
    fn pick_voice(&self) -> usize {
        if let Some((i, _)) = self.voices.iter().enumerate().find(|(_, v)| !v.is_active()) {
            return i;
        }
        // All busy: steal the oldest (smallest age).
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.age)
            .map_or(0, |(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// A clean patch: single sine oscillator, wide-open filter, instant attack
    /// and full sustain, so pitch and silence behavior are easy to assert.
    fn sine_patch() -> SynthParams {
        SynthParams {
            osc1: Waveform::Sine,
            osc2: Waveform::Sine,
            detune_cents: 0.0,
            mix: 0.0,
            cutoff: 20_000.0,
            resonance: 0.707,
            amp: AdsrParams {
                attack: 0.0,
                decay: 0.0,
                sustain: 1.0,
                release: 0.005,
            },
        }
    }

    #[test]
    fn plays_note_at_correct_pitch() {
        let mut synth = Synth::new(SR, sine_patch());
        synth.note_on(69, 1.0); // A4 = 440 Hz
        let mut prev = synth.tick();
        let mut crossings = 0;
        for _ in 0..(SR as usize) {
            let cur = synth.tick();
            if prev < 0.0 && cur >= 0.0 {
                crossings += 1;
            }
            prev = cur;
            assert!(cur.is_finite());
        }
        assert!(
            (438..=442).contains(&crossings),
            "A4 should be ~440 Hz, got {crossings}"
        );
    }

    #[test]
    fn goes_silent_after_release() {
        let mut synth = Synth::new(SR, sine_patch());
        synth.note_on(60, 1.0);
        for _ in 0..1000 {
            synth.tick();
        }
        assert_eq!(synth.active_voices(), 1);
        synth.note_off(60);
        // Release is 5 ms = 240 samples; run well past it.
        for _ in 0..1000 {
            synth.tick();
        }
        assert_eq!(synth.active_voices(), 0);
        assert_eq!(synth.tick(), 0.0);
    }

    #[test]
    fn eight_voice_polyphony_then_steals() {
        let mut synth = Synth::new(SR, sine_patch());
        for note in 60..68 {
            synth.note_on(note, 0.8);
        }
        synth.tick();
        assert_eq!(synth.active_voices(), VOICES);
        // A ninth note must steal, not exceed the voice budget.
        synth.note_on(72, 0.8);
        synth.tick();
        assert_eq!(synth.active_voices(), VOICES);
    }

    #[test]
    fn chord_output_is_finite_and_bounded() {
        let mut synth = Synth::new(SR, SynthParams::default());
        for note in [60, 64, 67, 72] {
            synth.note_on(note, 0.9);
        }
        for _ in 0..48_000 {
            let s = synth.tick();
            assert!(s.is_finite());
            assert!(s.abs() <= VOICES as f32, "output {s} exceeds voice budget");
        }
    }
}
