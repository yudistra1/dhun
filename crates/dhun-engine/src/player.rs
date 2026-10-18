//! The real-time playback engine (SPEC §3.1, §5).
//!
//! [`RealtimeEngine::prepare`] does all the allocation and scheduling up front;
//! [`RealtimeEngine::process`] runs on the audio thread and is strictly
//! allocation-, lock-, and I/O-free (CLAUDE.md real-time rules, proven by the
//! `no_alloc` guard test). Transport is controlled with `play`/`stop`/`rewind`.

use dhun_core::SampleRate;
use dhun_dsp::db_to_lin;
use dhun_project::Project;

use crate::error::EngineError;
use crate::render::{pan_gains, TAIL_SECONDS};
use crate::schedule::{collect_channel_events, song_total_samples, Timing};
use crate::voice::{build_voice, EventKind, Voice};

/// One mixer strip: a voice plus its precomputed left/right gains.
struct Strip {
    voice: Voice,
    lg: f32,
    rg: f32,
}

/// A scheduled event tagged with the strip it drives.
struct ChanEvent {
    at: usize,
    strip: usize,
    kind: EventKind,
}

/// A prepared, real-time-safe player for one project.
pub struct RealtimeEngine {
    sample_rate: u32,
    strips: Vec<Strip>,
    events: Vec<ChanEvent>,
    cursor: usize,
    position: usize,
    song_frames: usize,
    total_frames: usize,
    master_gain: f32,
    playing: bool,
    looping: bool,
}

impl RealtimeEngine {
    /// Prepares a player for a project. Allocates and schedules everything here
    /// so [`RealtimeEngine::process`] never needs to.
    ///
    /// `sample_rate` overrides the project's rate (e.g. with the audio device's
    /// actual rate); `None` uses the project's.
    ///
    /// # Errors
    /// Returns [`EngineError::Project`] if the project is invalid, or
    /// [`EngineError::UnsupportedSampleRate`] if the override rate is not
    /// supported.
    pub fn prepare(project: &Project, sample_rate: Option<u32>) -> Result<Self, EngineError> {
        project.validate()?;

        let sr = match sample_rate {
            Some(s) => SampleRate::new(s).map_err(|_| EngineError::UnsupportedSampleRate(s))?,
            None => SampleRate::new(project.sample_rate).expect("sample rate validated"),
        };
        let timing = Timing::new(project, sr);
        let song_frames = song_total_samples(project, &timing);
        let tail = (TAIL_SECONDS * f64::from(sr.get())) as usize;
        let total_frames = song_frames + tail;

        let any_solo = project.channels.iter().any(|c| c.mixer.solo);
        let mut strips = Vec::new();
        let mut events = Vec::new();

        for channel in &project.channels {
            if channel.mixer.mute || (any_solo && !channel.mixer.solo) {
                continue;
            }
            let strip = strips.len();
            let (lg, rg) = pan_gains(channel.mixer.gain_db, channel.mixer.pan);
            strips.push(Strip {
                voice: build_voice(&channel.instrument, sr.get() as f32),
                lg,
                rg,
            });
            for e in collect_channel_events(project, channel.id, &timing) {
                if e.at < total_frames {
                    events.push(ChanEvent {
                        at: e.at,
                        strip,
                        kind: e.kind,
                    });
                }
            }
        }
        events.sort_by_key(|e| e.at);

        Ok(Self {
            sample_rate: sr.get(),
            strips,
            events,
            cursor: 0,
            position: 0,
            song_frames,
            total_frames,
            master_gain: db_to_lin(project.master.gain_db),
            playing: false,
            looping: false,
        })
    }

    /// The sample rate this engine was prepared for.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Total frames in the render (song plus tail).
    #[must_use]
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Whether the transport is running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether non-looping playback has reached the end.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !self.looping && self.position >= self.total_frames
    }

    /// Starts (or resumes) playback from the current position.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pauses playback (position is kept).
    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Returns the playhead to the start.
    pub fn rewind(&mut self) {
        self.position = 0;
        self.cursor = 0;
    }

    /// Enables or disables looping over the song body (excluding the tail).
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Fills an interleaved stereo output buffer. **Real-time safe**: no
    /// allocation, locks, or I/O.
    pub fn process(&mut self, out: &mut [f32]) {
        let master = self.master_gain;
        for frame in out.chunks_exact_mut(2) {
            let limit = if self.looping {
                self.song_frames
            } else {
                self.total_frames
            };
            if self.playing && self.position >= limit {
                if self.looping {
                    self.position = 0;
                    self.cursor = 0;
                } else {
                    self.playing = false;
                }
            }
            if !self.playing {
                frame[0] = 0.0;
                frame[1] = 0.0;
                continue;
            }

            while self.cursor < self.events.len() && self.events[self.cursor].at == self.position {
                let kind = self.events[self.cursor].kind;
                let strip = self.events[self.cursor].strip;
                self.strips[strip].voice.apply(&kind);
                self.cursor += 1;
            }

            let mut l = 0.0f32;
            let mut r = 0.0f32;
            for s in &mut self.strips {
                let v = s.voice.tick();
                l += v * s.lg;
                r += v * s.rg;
            }
            frame[0] = l * master;
            frame[1] = r * master;
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_offline;
    use crate::test_fixtures::one_kick_project;

    #[test]
    fn realtime_matches_offline_render() {
        let project = one_kick_project();
        let offline = render_offline(&project, None).unwrap();

        let mut engine = RealtimeEngine::prepare(&project, None).unwrap();
        engine.play();
        let mut out = vec![0.0f32; engine.total_frames() * 2];
        // Process in device-sized blocks to mimic a real callback.
        for block in out.chunks_mut(512 * 2) {
            engine.process(block);
        }

        assert_eq!(out.len(), offline.interleaved.len());
        assert_eq!(
            out, offline.interleaved,
            "real-time path diverged from offline render"
        );
    }

    #[test]
    fn stopped_engine_outputs_silence() {
        let mut engine = RealtimeEngine::prepare(&one_kick_project(), None).unwrap();
        let mut out = vec![1.0f32; 256 * 2];
        engine.process(&mut out); // not playing yet
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn playback_finishes_and_stops() {
        let mut engine = RealtimeEngine::prepare(&one_kick_project(), None).unwrap();
        engine.play();
        let mut out = vec![0.0f32; engine.total_frames() * 2 + 1024];
        engine.process(&mut out);
        assert!(engine.is_finished());
        assert!(!engine.is_playing());
    }

    #[test]
    fn rewind_replays_from_start() {
        let mut engine = RealtimeEngine::prepare(&one_kick_project(), None).unwrap();
        engine.play();
        let mut a = vec![0.0f32; 480 * 2];
        engine.process(&mut a);
        engine.rewind();
        engine.play();
        let mut b = vec![0.0f32; 480 * 2];
        engine.process(&mut b);
        assert_eq!(a, b, "rewind did not reproduce the opening");
    }

    #[test]
    fn unsupported_device_rate_errors() {
        let err = RealtimeEngine::prepare(&one_kick_project(), Some(96_000));
        assert!(matches!(
            err,
            Err(EngineError::UnsupportedSampleRate(96_000))
        ));
    }
}
