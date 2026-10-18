//! Mixer insert effects: building `dhun-dsp` processors from project
//! [`EffectSpec`]s and applying them in a chain (SPEC §3.5).
//!
//! Chains are built off the audio thread (reverb allocates its delay lines in
//! `new`); applying them on the audio thread is allocation-free.

use dhun_dsp::{
    Compressor, CompressorParams, Effect, Gain, Reverb, ReverbParams, ThreeBandEq,
    ThreeBandEqParams,
};
use dhun_project::EffectSpec;

/// One concrete insert effect. An enum (not `dyn`) keeps dispatch concrete and
/// the type `Send` for the audio thread.
pub(crate) enum Insert {
    Gain(Gain),
    Eq(ThreeBandEq),
    Comp(Compressor),
    Reverb(Reverb),
}

impl Insert {
    /// Processes one sample through this effect.
    pub(crate) fn tick(&mut self, x: f32) -> f32 {
        match self {
            Insert::Gain(g) => g.tick(x),
            Insert::Eq(e) => e.tick(x),
            Insert::Comp(c) => c.tick(x),
            Insert::Reverb(r) => r.tick(x),
        }
    }
}

/// Builds an insert chain from a project effect spec list at `sample_rate`.
pub(crate) fn build_chain(specs: &[EffectSpec], sample_rate: f32) -> Vec<Insert> {
    specs.iter().map(|s| build_insert(s, sample_rate)).collect()
}

fn build_insert(spec: &EffectSpec, sample_rate: f32) -> Insert {
    match *spec {
        EffectSpec::Gain { gain_db } => Insert::Gain(Gain::from_db(gain_db)),
        EffectSpec::Eq {
            low_db,
            mid_db,
            high_db,
        } => Insert::Eq(ThreeBandEq::new(
            sample_rate,
            ThreeBandEqParams {
                low_db,
                mid_db,
                high_db,
            },
        )),
        EffectSpec::Compressor {
            threshold_db,
            ratio,
            attack,
            release,
            makeup_db,
        } => Insert::Comp(Compressor::new(
            sample_rate,
            CompressorParams {
                threshold_db,
                ratio: ratio.max(1.0),
                attack,
                release,
                makeup_db,
            },
        )),
        EffectSpec::Reverb {
            room_size,
            damping,
            wet,
            dry,
        } => Insert::Reverb(Reverb::new(
            sample_rate,
            ReverbParams {
                room_size: room_size.clamp(0.0, 1.0),
                damping: damping.clamp(0.0, 1.0),
                wet: wet.clamp(0.0, 1.0),
                dry: dry.clamp(0.0, 1.0),
            },
        )),
    }
}
