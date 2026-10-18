//! Real-time audit (SPEC §8.5): the audio-callback path must not allocate.
//!
//! A counting global allocator (installed only in this test binary) tallies
//! allocations while a flag is armed. We arm it around `RealtimeEngine::process`
//! and assert the count stays at zero — proving the callback path is
//! allocation-free (CLAUDE.md real-time rules).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use dhun_engine::RealtimeEngine;
use dhun_project::{
    Channel, DrumKind, DrumLane, EffectSpec, Instrument, Master, MixerStrip, Pattern, Project, Step,
};

struct CountingAlloc;

static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

// SAFETY: delegates every operation to the system allocator; the only added
// behavior is a relaxed counter increment when armed.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

/// A kick + closed-hat project so `process` exercises multiple strips and
/// drum triggers under the guard.
fn drum_project() -> Project {
    let step = |on: bool| Step { on, velocity: 0.9 };
    Project {
        format_version: 1,
        name: "No Alloc".to_string(),
        sample_rate: 48_000,
        tempo: 120.0,
        steps_per_beat: 4,
        channels: vec![
            Channel {
                id: 1,
                name: "Kick".to_string(),
                instrument: Instrument::Drum {
                    drum: DrumKind::Kick,
                },
                // A reverb insert so the guard also proves effects don't
                // allocate on the process path.
                mixer: MixerStrip {
                    effects: vec![EffectSpec::Reverb {
                        room_size: 0.5,
                        damping: 0.5,
                        wet: 0.3,
                        dry: 0.7,
                    }],
                    ..MixerStrip::default()
                },
            },
            Channel {
                id: 2,
                name: "Hat".to_string(),
                instrument: Instrument::Drum {
                    drum: DrumKind::ClosedHat,
                },
                mixer: MixerStrip::default(),
            },
        ],
        patterns: vec![Pattern {
            id: 1,
            name: "A".to_string(),
            steps: 4,
            drum_lanes: vec![
                DrumLane {
                    channel: 1,
                    steps: vec![step(true), step(false), step(false), step(false)],
                },
                DrumLane {
                    channel: 2,
                    steps: vec![step(true), step(true), step(true), step(true)],
                },
            ],
            note_lanes: vec![],
        }],
        song: vec![1],
        audio_tracks: vec![],
        master: Master::default(),
    }
}

#[test]
fn process_does_not_allocate() {
    let project = drum_project();
    let mut engine = RealtimeEngine::prepare(&project, None).expect("prepare");

    // Pre-allocate the output buffer and warm up once (flush any lazy init)
    // before arming the guard.
    let mut buf = vec![0.0f32; 512 * 2];
    engine.play();
    engine.process(&mut buf);
    engine.rewind();
    engine.play();

    ARMED.store(true, Ordering::Relaxed);
    for _ in 0..200 {
        engine.process(&mut buf);
    }
    ARMED.store(false, Ordering::Relaxed);

    let allocations = ALLOC_COUNT.load(Ordering::Relaxed);
    assert_eq!(
        allocations, 0,
        "audio callback path allocated {allocations} time(s)"
    );
}
