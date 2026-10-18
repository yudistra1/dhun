//! Turning a project's patterns and arrangement into a time-ordered list of
//! sample-accurate events. Shared by the offline renderer and the player.

use dhun_core::{Bpm, SampleRate, TempoMap, Ticks, PPQ};
use dhun_project::Project;

use crate::voice::EventKind;

/// An event tagged with its output sample position.
pub(crate) struct ScheduledEvent {
    /// Output frame at which the event fires.
    pub at: usize,
    /// What happens.
    pub kind: EventKind,
}

/// Tempo/grid context derived from a (validated) project.
pub(crate) struct Timing {
    map: TempoMap,
    ticks_per_step: u32,
}

impl Timing {
    /// Builds timing for a project at a given sample rate (which may differ
    /// from the project's own, e.g. the audio device's rate).
    pub(crate) fn new(project: &Project, sample_rate: SampleRate) -> Self {
        let tempo = Bpm::new(project.tempo).expect("tempo validated before Timing::new");
        Self {
            map: TempoMap::new(tempo, sample_rate),
            ticks_per_step: PPQ / project.steps_per_beat.max(1),
        }
    }

    /// Converts a tick position to the nearest output frame.
    pub(crate) fn tick_to_sample(&self, tick: u32) -> usize {
        let s = self.map.ticks_to_samples(Ticks::new(i64::from(tick))).get();
        usize::try_from(s).unwrap_or(0)
    }

    /// Converts a bar index (4/4) to an output frame.
    pub(crate) fn bar_to_frame(&self, bar: u32) -> usize {
        let bar_ticks = 4 * PPQ;
        let s = self
            .map
            .ticks_to_samples(Ticks::new(i64::from(bar) * i64::from(bar_ticks)))
            .get();
        usize::try_from(s).unwrap_or(0)
    }
}

/// Total length of the arranged song in output frames (no tail).
pub(crate) fn song_total_samples(project: &Project, timing: &Timing) -> usize {
    let mut song_ticks: u64 = 0;
    for &pid in &project.song {
        if let Some(pat) = project.pattern(pid) {
            song_ticks += u64::from(pat.steps) * u64::from(timing.ticks_per_step);
        }
    }
    timing.tick_to_sample(u32::try_from(song_ticks).unwrap_or(u32::MAX))
}

/// Collects one channel's events across the whole song timeline, in time order.
pub(crate) fn collect_channel_events(
    project: &Project,
    channel_id: u64,
    timing: &Timing,
) -> Vec<ScheduledEvent> {
    let mut events = Vec::new();
    let mut offset_ticks: u32 = 0;

    for &pid in &project.song {
        let Some(pat) = project.pattern(pid) else {
            continue;
        };
        for lane in &pat.drum_lanes {
            if lane.channel != channel_id {
                continue;
            }
            for (idx, step) in lane.steps.iter().enumerate() {
                if step.on {
                    let tick = offset_ticks + idx as u32 * timing.ticks_per_step;
                    events.push(ScheduledEvent {
                        at: timing.tick_to_sample(tick),
                        kind: EventKind::Drum {
                            velocity: step.velocity,
                        },
                    });
                }
            }
        }
        for lane in &pat.note_lanes {
            if lane.channel != channel_id {
                continue;
            }
            for note in &lane.notes {
                let on = offset_ticks + note.start_tick;
                let off = on + note.length_ticks;
                events.push(ScheduledEvent {
                    at: timing.tick_to_sample(on),
                    kind: EventKind::NoteOn {
                        pitch: note.pitch,
                        velocity: note.velocity,
                    },
                });
                events.push(ScheduledEvent {
                    at: timing.tick_to_sample(off),
                    kind: EventKind::NoteOff { pitch: note.pitch },
                });
            }
        }
        offset_ticks += pat.steps * timing.ticks_per_step;
    }
    events
}
