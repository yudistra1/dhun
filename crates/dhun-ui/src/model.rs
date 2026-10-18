//! The editable UI state: a [`Project`] plus transport/selection state, with
//! pure edit helpers that are unit-testable without egui.

use dhun_project::{
    Channel, DrumKind, DrumLane, Instrument, Master, MixerStrip, Note, NoteLane, Pattern, Project,
    Step, SynthSpec,
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
                Channel {
                    id: 4,
                    name: "Lead".to_string(),
                    instrument: Instrument::Synth {
                        params: SynthSpec::default(),
                    },
                    mixer: MixerStrip {
                        gain_db: -4.0,
                        ..MixerStrip::default()
                    },
                },
            ],
            patterns: vec![Pattern {
                id: 1,
                name: "Pattern 1".to_string(),
                steps,
                drum_lanes: vec![empty_lane(1), empty_lane(2), empty_lane(3)],
                note_lanes: vec![NoteLane {
                    channel: 4,
                    notes: vec![],
                }],
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

    /// The first synth channel's id, if any (the one the piano roll edits).
    #[must_use]
    pub fn first_synth_channel(&self) -> Option<u64> {
        self.project
            .channels
            .iter()
            .find(|c| matches!(c.instrument, Instrument::Synth { .. }))
            .map(|c| c.id)
    }

    /// Whether a one-step note exists at `pitch` on `step` in the current
    /// pattern's lane for `channel`.
    #[must_use]
    pub fn note_at(&self, channel: u64, pitch: u8, step: usize) -> bool {
        let start = step as u32 * self.project.ticks_per_step();
        self.current_pattern()
            .and_then(|p| p.note_lanes.iter().find(|l| l.channel == channel))
            .is_some_and(|l| {
                l.notes
                    .iter()
                    .any(|n| n.pitch == pitch && n.start_tick == start)
            })
    }

    /// Toggles a one-step note at `pitch`/`step` on `channel`'s note lane.
    /// Adding uses one step of length and [`DEFAULT_STEP_VELOCITY`].
    pub fn toggle_note(&mut self, channel: u64, pitch: u8, step: usize) {
        let tpq = self.project.ticks_per_step();
        let start = step as u32 * tpq;
        let Some(pattern) = self.project.patterns.get_mut(self.selected_pattern) else {
            return;
        };
        let Some(lane) = pattern.note_lanes.iter_mut().find(|l| l.channel == channel) else {
            return;
        };
        if let Some(pos) = lane
            .notes
            .iter()
            .position(|n| n.pitch == pitch && n.start_tick == start)
        {
            lane.notes.remove(pos);
        } else {
            lane.notes.push(Note {
                pitch,
                start_tick: start,
                length_ticks: tpq,
                velocity: DEFAULT_STEP_VELOCITY,
            });
        }
    }

    /// Overwrites the current pattern's drum lanes with a built-in [`Preset`],
    /// placing hits by each lane's drum kind.
    pub fn apply_preset(&mut self, preset: Preset) {
        let steps = self.current_pattern().map_or(0, |p| p.steps) as usize;
        // Map channel id -> its drum kind (immutable borrow finished before the
        // mutable one below).
        let kinds: Vec<(u64, DrumKind)> = self
            .project
            .channels
            .iter()
            .filter_map(|c| match c.instrument {
                Instrument::Drum { drum } => Some((c.id, drum)),
                Instrument::Synth { .. } => None,
            })
            .collect();

        let Some(pattern) = self.project.patterns.get_mut(self.selected_pattern) else {
            return;
        };
        for lane in &mut pattern.drum_lanes {
            let Some(&(_, kind)) = kinds.iter().find(|(id, _)| *id == lane.channel) else {
                continue;
            };
            let active = preset.active_steps(kind, steps);
            for (i, cell) in lane.steps.iter_mut().enumerate() {
                cell.on = active.contains(&i);
                cell.velocity = if cell.on { DEFAULT_STEP_VELOCITY } else { 0.0 };
            }
        }
    }
}

/// Built-in arrangement presets for the step sequencer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Four-on-the-floor house groove.
    FourOnTheFloor,
    /// Classic boom-bap backbeat.
    BoomBap,
    /// Trap-style pattern with busy hats.
    Trap,
    /// Clear every step.
    Clear,
}

impl Preset {
    /// All presets, in display order.
    pub const ALL: [Preset; 4] = [
        Preset::FourOnTheFloor,
        Preset::BoomBap,
        Preset::Trap,
        Preset::Clear,
    ];

    /// A short label for buttons.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Preset::FourOnTheFloor => "Four on the Floor",
            Preset::BoomBap => "Boom Bap",
            Preset::Trap => "Trap",
            Preset::Clear => "Clear",
        }
    }

    /// Which step indices are on for a given drum kind (patterns are authored on
    /// a 16-step grid and clamped to `steps`).
    #[must_use]
    pub fn active_steps(self, kind: DrumKind, steps: usize) -> Vec<usize> {
        let grid: &[usize] = match (self, kind) {
            (Preset::Clear, _) => &[],

            (Preset::FourOnTheFloor, DrumKind::Kick) => &[0, 4, 8, 12],
            (Preset::FourOnTheFloor, DrumKind::Snare | DrumKind::Clap) => &[4, 12],
            (Preset::FourOnTheFloor, DrumKind::ClosedHat) => &[2, 6, 10, 14],
            (Preset::FourOnTheFloor, DrumKind::OpenHat) => &[2, 6, 10, 14],

            (Preset::BoomBap, DrumKind::Kick) => &[0, 10],
            (Preset::BoomBap, DrumKind::Snare | DrumKind::Clap) => &[4, 12],
            (Preset::BoomBap, DrumKind::ClosedHat) => &[0, 2, 4, 6, 8, 10, 12, 14],
            (Preset::BoomBap, DrumKind::OpenHat) => &[14],

            (Preset::Trap, DrumKind::Kick) => &[0, 6, 10],
            (Preset::Trap, DrumKind::Snare | DrumKind::Clap) => &[8],
            (Preset::Trap, DrumKind::ClosedHat) => &[0, 2, 4, 6, 8, 10, 11, 12, 14, 15],
            (Preset::Trap, DrumKind::OpenHat) => &[7],

            // Toms are left untouched by these grooves.
            (_, DrumKind::Tom) => &[],
        };
        grid.iter().copied().filter(|&i| i < steps).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_drum_and_synth_channels_and_one_empty_pattern() {
        let state = EditorState::demo();
        // Three drums + one synth.
        assert_eq!(state.project.channels.len(), 4);
        let drums = state
            .project
            .channels
            .iter()
            .filter(|c| matches!(c.instrument, Instrument::Drum { .. }))
            .count();
        assert_eq!(drums, 3);
        assert_eq!(state.project.patterns.len(), 1);
        assert_eq!(state.project.patterns[0].steps, 16);
        // Every drum step starts off, and there are no notes yet.
        assert!(state.project.patterns[0]
            .drum_lanes
            .iter()
            .all(|l| l.steps.iter().all(|s| !s.on)));
        assert!(state.project.patterns[0]
            .note_lanes
            .iter()
            .all(|l| l.notes.is_empty()));
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
    fn demo_has_a_synth_channel_with_a_note_lane() {
        let state = EditorState::demo();
        assert_eq!(state.first_synth_channel(), Some(4));
        assert!(state.project.patterns[0]
            .note_lanes
            .iter()
            .any(|l| l.channel == 4));
    }

    #[test]
    fn toggle_note_adds_then_removes() {
        let mut state = EditorState::demo();
        let ch = state.first_synth_channel().unwrap();
        assert!(!state.note_at(ch, 60, 2));
        state.toggle_note(ch, 60, 2);
        assert!(state.note_at(ch, 60, 2));
        // Placed on the grid at step 2 with one step of length.
        let tpq = state.project.ticks_per_step();
        let note = state.project.patterns[0].note_lanes[0].notes[0];
        assert_eq!(note.pitch, 60);
        assert_eq!(note.start_tick, 2 * tpq);
        assert_eq!(note.length_ticks, tpq);
        state.toggle_note(ch, 60, 2);
        assert!(!state.note_at(ch, 60, 2));
        state.project.validate().unwrap();
    }

    #[test]
    fn preset_fills_lanes_by_drum_kind() {
        let mut state = EditorState::demo();
        state.apply_preset(Preset::FourOnTheFloor);
        // Kick on the quarter notes; the project stays valid.
        for step in [0, 4, 8, 12] {
            assert!(state.step_on(1, step), "kick should be on at {step}");
        }
        assert!(!state.step_on(1, 1));
        assert!(state.step_on(2, 4), "snare on beat 2");
        state.project.validate().unwrap();
    }

    #[test]
    fn clear_preset_empties_every_lane() {
        let mut state = EditorState::demo();
        state.apply_preset(Preset::Trap);
        state.apply_preset(Preset::Clear);
        assert!(state.project.patterns[0]
            .drum_lanes
            .iter()
            .all(|l| l.steps.iter().all(|s| !s.on)));
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
