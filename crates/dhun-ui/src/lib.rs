//! `dhun-ui` — Dhun's egui views: transport, step sequencer, and mixer
//! (SPEC §3.1, §3.2, §3.5).
//!
//! This crate depends only on `dhun-project` and `egui` (SPEC §6). It owns the
//! editable [`EditorState`] and renders it with [`draw`], which returns
//! [`UiCommand`]s — the *only* way it communicates with the engine. The host
//! (`dhun-app`) owns the engine and acts on those commands.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod command;
pub mod model;
pub mod theme;
pub mod view;

pub use command::UiCommand;
pub use model::{EditorState, Preset};
pub use theme::apply_theme;
pub use view::draw;
