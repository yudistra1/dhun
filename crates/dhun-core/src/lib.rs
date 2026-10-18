//! `dhun-core` — the shared vocabulary every higher Dhun layer speaks.
//!
//! This crate holds musical and audio [`time`] ([`Ticks`], [`Samples`],
//! [`SampleRate`], [`PPQ`]), [`tempo`] types ([`Bpm`], [`TimeSignature`],
//! [`TempoMap`]), entity [`ids`], and the shared [`error`] type ([`CoreError`]).
//!
//! It has **no dependency on any other Dhun crate** (SPEC §6) and contains only
//! pure, allocation-free, unit-testable types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod ids;
pub mod tempo;
pub mod time;

pub use error::CoreError;
pub use ids::{ChannelId, ClipId, NoteId, PatternId, TrackId};
pub use tempo::{Bpm, TempoMap, TimeSignature};
pub use time::{SampleRate, Samples, Ticks, PPQ};
