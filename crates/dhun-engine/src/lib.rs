//! `dhun-engine` — scheduling, voices, and the offline renderer.
//!
//! v0.1 lands the deterministic offline [`render_offline`] path (SPEC §3.6);
//! real-time playback via `cpal` arrives in a later milestone. The renderer
//! turns a [`dhun_project::Project`] into interleaved stereo audio by driving
//! `dhun-dsp` instruments through a sample-accurate event schedule.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod render;

pub use error::EngineError;
pub use render::{render_offline, RenderRange, RenderedAudio};
