//! `dhun-engine` — scheduling, voices, offline render, and real-time playback.
//!
//! Two paths share the same scheduling ([`schedule`]) and DSP voices
//! ([`voice`]): [`render_offline`] produces a deterministic buffer off-thread
//! (SPEC §3.6), and [`RealtimeEngine`] drives the same voices from an
//! allocation-free `process` on the audio thread (SPEC §5). Real-time device
//! I/O lives one layer up in `dhun-io`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audio;
pub mod error;
pub mod meters;
pub mod player;
pub mod render;

mod inserts;
mod schedule;
mod voice;

pub use audio::{AudioAssets, ClipData};
pub use error::EngineError;
pub use meters::EngineMeters;
pub use player::{Program, RealtimeEngine};
pub use render::{render_offline, render_offline_with, RenderRange, RenderedAudio};

// Re-exported so downstream crates (e.g. `dhun-io`) can name the engine's input
// type without taking a direct dependency on `dhun-project` (SPEC §6).
pub use dhun_project::Project;

#[cfg(test)]
pub(crate) mod test_fixtures {
    //! Shared project fixtures for engine tests.

    use dhun_project::{
        Channel, DrumKind, DrumLane, Instrument, MixerStrip, Pattern, Project, Step,
    };

    /// A one-bar project with a single kick on beat 1.
    pub(crate) fn one_kick_project() -> Project {
        Project {
            sample_rate: 48_000,
            tempo: 120.0,
            steps_per_beat: 4,
            channels: vec![Channel {
                id: 1,
                name: "Kick".to_string(),
                instrument: Instrument::Drum {
                    drum: DrumKind::Kick,
                },
                mixer: MixerStrip::default(),
            }],
            patterns: vec![Pattern {
                id: 1,
                name: "A".to_string(),
                steps: 4,
                drum_lanes: vec![DrumLane {
                    channel: 1,
                    steps: vec![
                        Step {
                            on: true,
                            velocity: 1.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                        Step {
                            on: false,
                            velocity: 0.0,
                        },
                    ],
                }],
                note_lanes: vec![],
            }],
            song: vec![1],
            ..Project::default()
        }
    }
}
