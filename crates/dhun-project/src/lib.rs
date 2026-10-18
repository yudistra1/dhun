//! `dhun-project` — the Dhun project model, serialization, and migrations
//! (SPEC §3.6).
//!
//! A project serializes to `project.json`. Loading is total: any malformed,
//! truncated, or out-of-range input returns a [`ProjectError`], never a panic
//! (SPEC §5, §8.4). The on-disk format carries a [`CURRENT_FORMAT_VERSION`] and
//! a forward-migration hook.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod model;

use std::path::Path;

pub use error::ProjectError;
pub use model::{
    AdsrSpec, Channel, DrumKind, DrumLane, Instrument, Master, MixerStrip, Note, NoteLane, Pattern,
    Project, Step, SynthSpec, WaveformSpec, CURRENT_FORMAT_VERSION,
};

impl Project {
    /// Parses a project from a JSON string, applying forward migrations.
    ///
    /// Does not fully validate semantics — call [`Project::validate`] for that.
    ///
    /// # Errors
    /// Returns [`ProjectError::Parse`] on malformed JSON and
    /// [`ProjectError::UnsupportedVersion`] if the file is newer than this build.
    pub fn from_json(json: &str) -> Result<Self, ProjectError> {
        let project: Project = serde_json::from_str(json)?;
        migrate(project)
    }

    /// Serializes the project to pretty-printed JSON.
    ///
    /// # Errors
    /// Returns [`ProjectError::Parse`] if serialization fails (should not happen
    /// for a well-formed in-memory project).
    pub fn to_json(&self) -> Result<String, ProjectError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Loads and validates a project from `<dir>/project.json` or a direct file
    /// path.
    ///
    /// # Errors
    /// Returns a [`ProjectError`] on I/O failure, parse failure, an unsupported
    /// version, or a failed validation.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let path = path.as_ref();
        let file = if path.is_dir() {
            path.join("project.json")
        } else {
            path.to_path_buf()
        };
        let json = std::fs::read_to_string(file)?;
        let project = Self::from_json(&json)?;
        project.validate()?;
        Ok(project)
    }

    /// Saves the project to `<dir>/project.json` (creating `dir`) or to a direct
    /// file path.
    ///
    /// # Errors
    /// Returns a [`ProjectError`] on serialization or I/O failure.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        let path = path.as_ref();
        let file = if path.extension().is_some() {
            path.to_path_buf()
        } else {
            std::fs::create_dir_all(path)?;
            path.join("project.json")
        };
        std::fs::write(file, self.to_json()?)?;
        Ok(())
    }

    /// Checks the project for internal consistency and value ranges.
    ///
    /// # Errors
    /// Returns [`ProjectError::Validation`] describing the first problem found,
    /// or [`ProjectError::UnsupportedVersion`] for a future format version.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.format_version > CURRENT_FORMAT_VERSION {
            return Err(ProjectError::UnsupportedVersion {
                found: self.format_version,
                supported: CURRENT_FORMAT_VERSION,
            });
        }

        dhun_core::SampleRate::new(self.sample_rate)
            .map_err(|e| ProjectError::Validation(e.to_string()))?;
        dhun_core::Bpm::new(self.tempo).map_err(|e| ProjectError::Validation(e.to_string()))?;

        if self.steps_per_beat == 0 {
            return Err(ProjectError::Validation(
                "steps_per_beat must be at least 1".to_string(),
            ));
        }

        // Channel ids must be unique.
        let mut ids = std::collections::HashSet::new();
        for ch in &self.channels {
            if !ids.insert(ch.id) {
                return Err(ProjectError::Validation(format!(
                    "duplicate channel id {}",
                    ch.id
                )));
            }
        }

        for pat in &self.patterns {
            if pat.steps == 0 {
                return Err(ProjectError::Validation(format!(
                    "pattern {} has zero steps",
                    pat.id
                )));
            }
            for lane in &pat.drum_lanes {
                let ch = self.channel(lane.channel).ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "drum lane references unknown channel {}",
                        lane.channel
                    ))
                })?;
                if !matches!(ch.instrument, Instrument::Drum { .. }) {
                    return Err(ProjectError::Validation(format!(
                        "drum lane on non-drum channel {}",
                        lane.channel
                    )));
                }
                if lane.steps.len() as u32 != pat.steps {
                    return Err(ProjectError::Validation(format!(
                        "drum lane on channel {} has {} steps, pattern has {}",
                        lane.channel,
                        lane.steps.len(),
                        pat.steps
                    )));
                }
            }
            for lane in &pat.note_lanes {
                let ch = self.channel(lane.channel).ok_or_else(|| {
                    ProjectError::Validation(format!(
                        "note lane references unknown channel {}",
                        lane.channel
                    ))
                })?;
                if !matches!(ch.instrument, Instrument::Synth { .. }) {
                    return Err(ProjectError::Validation(format!(
                        "note lane on non-synth channel {}",
                        lane.channel
                    )));
                }
            }
        }

        for id in &self.song {
            if self.pattern(*id).is_none() {
                return Err(ProjectError::Validation(format!(
                    "song references unknown pattern {id}"
                )));
            }
        }

        Ok(())
    }

    /// Ticks per sequencer step, derived from the timing resolution and
    /// `steps_per_beat` (e.g. 960 PPQ / 4 = 240 ticks per sixteenth-note step).
    #[must_use]
    pub fn ticks_per_step(&self) -> u32 {
        dhun_core::PPQ / self.steps_per_beat.max(1)
    }

    /// Looks up a channel by id.
    #[must_use]
    pub fn channel(&self, id: u64) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Looks up a pattern by id.
    #[must_use]
    pub fn pattern(&self, id: u64) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }
}

/// Applies forward migrations to bring a parsed project up to the current
/// format version.
fn migrate(mut project: Project) -> Result<Project, ProjectError> {
    if project.format_version > CURRENT_FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: project.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    // No intermediate versions exist yet; future migrations step through here.
    project.format_version = CURRENT_FORMAT_VERSION;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> Project {
        Project {
            name: "Round Trip".to_string(),
            tempo: 128.0,
            channels: vec![
                Channel {
                    id: 1,
                    name: "Kick".to_string(),
                    instrument: Instrument::Drum {
                        drum: DrumKind::Kick,
                    },
                    mixer: MixerStrip::default(),
                },
                Channel {
                    id: 2,
                    name: "Lead".to_string(),
                    instrument: Instrument::Synth {
                        params: SynthSpec::default(),
                    },
                    mixer: MixerStrip {
                        gain_db: -3.0,
                        pan: 0.25,
                        mute: false,
                        solo: false,
                    },
                },
            ],
            patterns: vec![Pattern {
                id: 10,
                name: "A".to_string(),
                steps: 8,
                drum_lanes: vec![DrumLane {
                    channel: 1,
                    steps: (0..8)
                        .map(|i| Step {
                            on: i % 2 == 0,
                            velocity: 0.9,
                        })
                        .collect(),
                }],
                note_lanes: vec![NoteLane {
                    channel: 2,
                    notes: vec![Note {
                        pitch: 60,
                        start_tick: 0,
                        length_ticks: 240,
                        velocity: 0.8,
                    }],
                }],
            }],
            song: vec![10],
            ..Project::default()
        }
    }

    #[test]
    fn round_trip_is_identical() {
        let p = sample_project();
        p.validate().unwrap();
        let s1 = p.to_json().unwrap();
        let p2 = Project::from_json(&s1).unwrap();
        let s2 = p2.to_json().unwrap();
        assert_eq!(p, p2, "parsed project differs from original");
        assert_eq!(s1, s2, "re-serialized JSON differs (SPEC §8.3)");
    }

    #[test]
    fn malformed_json_errors_without_panic() {
        for bad in [
            "",
            "{",
            "null",
            "[]",
            "{\"format_version\": 1}",     // missing required fields
            "{\"format_version\": \"x\"}", // wrong type
            "not json at all",
        ] {
            assert!(
                Project::from_json(bad).is_err(),
                "expected error for input {bad:?}"
            );
        }
    }

    #[test]
    fn future_version_is_rejected() {
        let mut p = sample_project();
        p.format_version = CURRENT_FORMAT_VERSION + 1;
        let json = serde_json::to_string(&p).unwrap();
        assert!(matches!(
            Project::from_json(&json),
            Err(ProjectError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn validate_rejects_bad_values_and_dangling_refs() {
        let mut p = sample_project();
        p.sample_rate = 96_000;
        assert!(p.validate().is_err(), "bad sample rate");

        let mut p = sample_project();
        p.tempo = 1_000.0;
        assert!(p.validate().is_err(), "bad tempo");

        let mut p = sample_project();
        p.song = vec![999];
        assert!(p.validate().is_err(), "dangling song reference");

        let mut p = sample_project();
        p.patterns[0].drum_lanes[0].channel = 2; // a synth channel
        assert!(p.validate().is_err(), "drum lane on synth channel");
    }
}
