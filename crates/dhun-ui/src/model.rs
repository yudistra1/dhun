//! The editable UI state: a [`Project`] plus transport/selection state, with
//! pure edit helpers that are unit-testable without egui.

use dhun_project::{
    Channel, DrumKind, DrumLane, Instrument, Master, MixerStrip, Pattern, Project, Step,
};

/// Default velocity applied when a step is switched on.
pub const DEFAULT_STEP_VELOCITY: f32 = 0.9;

/// Everything the editor needs: the project being built plus UI-only state.
pub struct EditorState {
    /// The project being edited and played.
    pub project: Project,
    /// Whether the transport is (believed to be) playing.
    pub playing: bool,
    /// Whether looping is enabled.
    pub looping: bool,
    /// Index into `project.patterns` currently shown in the sequencer.
    pub selected_pattern: usize,
}

impl EditorState {
    /// Wraps an existing project.
    #[must_use]
    pub fn new(project: Project) -> Self {
        Self {
            project,
            playing: false,
            looping: false,
            selected_pattern: 0,
        }
    }

    /// A starter kit: kick / snare / closed-hat channels and one empty 16-step
    /// pattern, ready for the user to click in a beat and press play.
    #[must_use]
    pub fn demo() -> Self {
        let steps = 16;
        let empty_lane = |channel: u64| DrumLane {
            channel,
            steps: vec![
                Step {
                    on: false,
                    velocity: 0.0,
                };
                steps as usize
            ],
        };
        let drum = |id: u64, name: &str, kind: DrumKind, gain_db: f32| Channel {
            id,
            name: name.to_string(),
            instrument: Instrument::Drum { drum: kind },
            mixer: MixerStrip {
                gain_db,
                ..MixerStrip::default()
            },
        };

        let project = Project {
            name: "New Beat".to_string(),
            sample_rate: 48_000,
            tempo: 120.0,
            steps_per_beat: 4,
            channels: vec![
                drum(1, "Kick", DrumKind::Kick, 0.0),
                drum(2, "Snare", DrumKind::Snare, -2.0),
                drum(3, "Closed Hat", DrumKind::ClosedHat, -6.0),
            ],
            patterns: vec![Pattern {
                id: 1,
                name: "Pattern 1".to_string(),
                steps,
                drum_lanes: vec![empty_lane(1), empty_lane(2), empty_lane(3)],
                note_lanes: vec![],
            }],
            song: vec![1],
            master: Master::default(),
            ..Project::default()
        };

        Self::new(project)
    }

    /// The pattern currently shown in the sequencer, if any.
    #[must_use]
    pub fn current_pattern(&self) -> Option<&Pattern> {
        self.project.patterns.get(self.selected_pattern)
    }

    /// Toggles the on/off state of a step in the current pattern's lane for
    /// `channel`. Turning a step on gives it [`DEFAULT_STEP_VELOCITY`].
    pub fn toggle_step(&mut self, channel: u64, step: usize) {
        let Some(pattern) = self.project.patterns.get_mut(self.selected_pattern) else {
            return;
        };
        let Some(lane) = pattern.drum_lanes.iter_mut().find(|l| l.channel == channel) else {
            return;
        };
        let Some(cell) = lane.steps.get_mut(step) else {
            return;
        };
        cell.on = !cell.on;
        cell.velocity = if cell.on { DEFAULT_STEP_VELOCITY } else { 0.0 };
    }

    /// Whether the given step is currently on.
    #[must_use]
    pub fn step_on(&self, channel: u64, step: usize) -> bool {
        self.current_pattern()
            .and_then(|p| p.drum_lanes.iter().find(|l| l.channel == channel))
            .and_then(|l| l.steps.get(step))
            .is_some_and(|c| c.on)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_three_drum_channels_and_one_empty_pattern() {
        let state = EditorState::demo();
        assert_eq!(state.project.channels.len(), 3);
        assert_eq!(state.project.patterns.len(), 1);
        assert_eq!(state.project.patterns[0].steps, 16);
        // Every step starts off.
        assert!(state.project.patterns[0]
            .drum_lanes
            .iter()
            .all(|l| l.steps.iter().all(|s| !s.on)));
        // And it is a valid, renderable project.
        state
            .project
            .validate()
            .expect("demo project should validate");
    }

    #[test]
    fn toggle_step_flips_and_sets_velocity() {
        let mut state = EditorState::demo();
        assert!(!state.step_on(1, 0));
        state.toggle_step(1, 0);
        assert!(state.step_on(1, 0));
        let vel = state.project.patterns[0].drum_lanes[0].steps[0].velocity;
        assert!((vel - DEFAULT_STEP_VELOCITY).abs() < f32::EPSILON);
        state.toggle_step(1, 0);
        assert!(!state.step_on(1, 0));
        assert_eq!(
            state.project.patterns[0].drum_lanes[0].steps[0].velocity,
            0.0
        );
    }

    #[test]
    fn toggle_step_ignores_unknown_channel_or_step() {
        let mut state = EditorState::demo();
        state.toggle_step(999, 0); // no such channel
        state.toggle_step(1, 999); // out-of-range step
                                   // Nothing turned on.
        assert!(state.project.patterns[0]
            .drum_lanes
            .iter()
            .all(|l| l.steps.iter().all(|s| !s.on)));
    }
}
