//! Mixer effects (SPEC §3.5 v0.1 minimum): [`Gain`], [`ThreeBandEq`],
//! [`Compressor`], and [`Reverb`].
//!
//! Each effect implements [`Effect`]: a per-sample `tick` plus a block `process`
//! that never allocates (reverb's delay lines are sized at construction).

pub mod compressor;
pub mod eq;
pub mod gain;
pub mod reverb;

pub use compressor::Compressor;
pub use eq::{ThreeBandEq, ThreeBandEqParams};
pub use gain::Gain;
pub use reverb::Reverb;

/// A mono audio processor applied one sample (or one block) at a time.
pub trait Effect {
    /// Processes a single input sample and returns the output.
    fn tick(&mut self, input: f32) -> f32;

    /// Clears any internal state (delay lines, filter memory, envelopes).
    fn reset(&mut self);

    /// Processes a block in place. Default implementation calls [`Effect::tick`]
    /// per sample; allocation-free.
    fn process(&mut self, buffer: &mut [f32]) {
        for s in buffer.iter_mut() {
            *s = self.tick(*s);
        }
    }
}
