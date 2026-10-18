//! Commands the UI emits for the host app to act on.
//!
//! `dhun-ui` never touches the engine directly (SPEC §6); it returns these from
//! [`crate::draw`], and `dhun-app` translates them into transport actions.

/// A transport command produced by a UI interaction this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    /// Start (or restart) playback of the current project.
    Play,
    /// Stop playback.
    Stop,
    /// Return the playhead to the start.
    Rewind,
    /// Enable or disable looping.
    SetLooping(bool),
}
