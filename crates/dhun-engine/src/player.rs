//! The real-time playback engine (SPEC §3.1, §5).
//!
//! [`RealtimeEngine::prepare`] does all allocation and scheduling up front;
//! [`RealtimeEngine::process`] runs on the audio thread and is strictly
//! allocation-, lock-, and I/O-free (proven by the `no_alloc` guard test).
//!
//! Live editing: a fresh [`Program`] is built off the audio thread and handed
//! in with [`RealtimeEngine::load_program`], which swaps it while keeping the
//! playhead and ringing voices, and returns the old program for the caller to
//! drop off-thread. So step, mixer, and tempo edits are heard as the loop
//! plays, without ever allocating or freeing on the audio thread.

use std::sync::Arc;

use dhun_core::SampleRate;
use dhun_dsp::db_to_lin;
use dhun_project::Project;

use crate::audio::{AudioAssets, ClipData};
use crate::error::EngineError;
use crate::inserts::{build_chain, Insert};
use crate::render::{pan_gains, TAIL_SECONDS};
use crate::schedule::{collect_channel_events, song_total_samples, Timing};
use crate::voice::{build_voice, EventKind, Voice};

/// A scheduled event tagged with the voice (strip) it drives.
struct ChanEvent {
    at: usize,
    strip: usize,
    kind: EventKind,
}

/// A placed audio clip ready for sample-accurate playback.
struct AudioPlayback {
    /// First output frame at which the clip sounds.
    start: usize,
    /// One past the last output frame.
    end: usize,
    /// Read offset into the source (trim / latency compensation).
    offset: usize,
    /// Left/right gains (track mix × clip gain, mute/solo applied).
    lg: f32,
    rg: f32,
    data: Arc<ClipData>,
}

/// A swappable snapshot of what to play: the scheduled events plus per-voice and
/// master mix. Built from a [`Project`] off the audio thread.
///
/// Voices live in the [`RealtimeEngine`] (they ring across edits); a `Program`
/// carries only data that an edit can change, indexed to match the engine's
/// voices by channel position.
pub struct Program {
    events: Vec<ChanEvent>,
    /// Effective (left, right) gain per channel, with mute/solo already applied.
    gains: Vec<(f32, f32)>,
    /// Per-channel insert chains, aligned with `gains` / the engine's voices.
    channel_inserts: Vec<Vec<Insert>>,
    audio: Vec<AudioPlayback>,
    /// Master insert chains, one per stereo side.
    master_l: Vec<Insert>,
    master_r: Vec<Insert>,
    master: f32,
    song_frames: usize,
    total_frames: usize,
}

impl Program {
    /// Builds a program from a project at the given sample rate, with no audio
    /// clips loaded.
    ///
    /// # Errors
    /// Returns [`EngineError::UnsupportedSampleRate`] if the rate is not
    /// supported.
    pub fn build(project: &Project, sample_rate: u32) -> Result<Program, EngineError> {
        Self::build_with(project, sample_rate, &AudioAssets::empty())
    }

    /// Builds a program, placing audio clips from `assets`. Allocates — call off
    /// the audio thread.
    ///
    /// # Errors
    /// Returns [`EngineError::UnsupportedSampleRate`] if the rate is not
    /// supported.
    pub fn build_with(
        project: &Project,
        sample_rate: u32,
        assets: &AudioAssets,
    ) -> Result<Program, EngineError> {
        let sr = SampleRate::new(sample_rate)
            .map_err(|_| EngineError::UnsupportedSampleRate(sample_rate))?;
        let timing = Timing::new(project, sr);
        let song_frames = song_total_samples(project, &timing);
        let tail = (TAIL_SECONDS * f64::from(sr.get())) as usize;
        let mut total_frames = song_frames + tail;

        // Solo across both instrument channels and audio tracks.
        let any_solo = project.channels.iter().any(|c| c.mixer.solo)
            || project.audio_tracks.iter().any(|t| t.mixer.solo);

        let sr_f = sr.get() as f32;
        let mut gains = Vec::with_capacity(project.channels.len());
        let mut channel_inserts = Vec::with_capacity(project.channels.len());
        let mut events = Vec::new();

        for (i, ch) in project.channels.iter().enumerate() {
            let audible = !ch.mixer.mute && (!any_solo || ch.mixer.solo);
            gains.push(if audible {
                pan_gains(ch.mixer.gain_db, ch.mixer.pan)
            } else {
                (0.0, 0.0)
            });
            channel_inserts.push(build_chain(&ch.mixer.effects, sr_f));
            for e in collect_channel_events(project, ch.id, &timing) {
                if e.at < total_frames {
                    events.push(ChanEvent {
                        at: e.at,
                        strip: i,
                        kind: e.kind,
                    });
                }
            }
        }
        events.sort_by_key(|e| e.at);

        // Place audio clips.
        let mut audio = Vec::new();
        for track in &project.audio_tracks {
            let audible = !track.mixer.mute && (!any_solo || track.mixer.solo);
            let (tlg, trg) = if audible {
                pan_gains(track.mixer.gain_db, track.mixer.pan)
            } else {
                (0.0, 0.0)
            };
            for clip in &track.clips {
                let Some(data) = assets.get(&clip.source) else {
                    continue; // clip not loaded (e.g. device playback) — skip
                };
                let offset = (clip.offset_frames as usize).min(data.len());
                let available = data.len() - offset;
                let len = if clip.length_frames == 0 {
                    available
                } else {
                    (clip.length_frames as usize).min(available)
                };
                let start = timing.tick_to_sample(clip.start_tick);
                let end = start + len;
                total_frames = total_frames.max(end);
                let clip_gain = db_to_lin(clip.gain_db);
                audio.push(AudioPlayback {
                    start,
                    end,
                    offset,
                    lg: tlg * clip_gain,
                    rg: trg * clip_gain,
                    data,
                });
            }
        }

        Ok(Program {
            events,
            gains,
            channel_inserts,
            audio,
            master_l: build_chain(&project.master.effects, sr_f),
            master_r: build_chain(&project.master.effects, sr_f),
            master: db_to_lin(project.master.gain_db),
            song_frames,
            total_frames,
        })
    }

    /// Number of channels (voices) this program expects.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.gains.len()
    }
}

/// A prepared, real-time-safe player for one project.
pub struct RealtimeEngine {
    sample_rate: u32,
    voices: Vec<Voice>,
    program: Box<Program>,
    cursor: usize,
    position: usize,
    playing: bool,
    looping: bool,
}

impl RealtimeEngine {
    /// Prepares a player for a project. Allocates voices and the initial
    /// program here so [`RealtimeEngine::process`] never needs to.
    ///
    /// `sample_rate` overrides the project's rate (e.g. the audio device's);
    /// `None` uses the project's.
    ///
    /// # Errors
    /// Returns [`EngineError::Project`] if the project is invalid, or
    /// [`EngineError::UnsupportedSampleRate`] if the override rate is not
    /// supported.
    pub fn prepare(project: &Project, sample_rate: Option<u32>) -> Result<Self, EngineError> {
        Self::prepare_with(project, sample_rate, &AudioAssets::empty())
    }

    /// Like [`RealtimeEngine::prepare`], but with audio clips loaded from
    /// `assets` so recorded tracks play back.
    ///
    /// # Errors
    /// As [`RealtimeEngine::prepare`].
    pub fn prepare_with(
        project: &Project,
        sample_rate: Option<u32>,
        assets: &AudioAssets,
    ) -> Result<Self, EngineError> {
        project.validate()?;
        let sr = match sample_rate {
            Some(s) => SampleRate::new(s).map_err(|_| EngineError::UnsupportedSampleRate(s))?,
            None => SampleRate::new(project.sample_rate).expect("sample rate validated"),
        };
        let voices = project
            .channels
            .iter()
            .map(|c| build_voice(&c.instrument, sr.get() as f32))
            .collect();
        let program = Box::new(Program::build_with(project, sr.get(), assets)?);

        Ok(Self {
            sample_rate: sr.get(),
            voices,
            program,
            cursor: 0,
            position: 0,
            playing: false,
            looping: false,
        })
    }

    /// The sample rate this engine was prepared for.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of channels (voices).
    #[must_use]
    pub fn channels(&self) -> usize {
        self.voices.len()
    }

    /// Total frames in the current program (song plus tail).
    #[must_use]
    pub fn total_frames(&self) -> usize {
        self.program.total_frames
    }

    /// Whether the transport is running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Whether non-looping playback has reached the end.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !self.looping && self.position >= self.program.total_frames
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

    /// Swaps in a freshly built program, keeping the playhead and ringing
    /// voices, and returns the previous program for the caller to drop off the
    /// audio thread.
    ///
    /// If the new program's channel count does not match the engine's voices
    /// (the channel set changed), it is rejected and handed straight back —
    /// recreate the engine for structural changes.
    ///
    /// Allocation-free: a pointer swap plus a binary search for the cursor.
    pub fn load_program(&mut self, new: Box<Program>) -> Box<Program> {
        if new.channels() != self.voices.len() {
            return new;
        }
        let old = std::mem::replace(&mut self.program, new);
        self.cursor = self
            .program
            .events
            .partition_point(|e| e.at < self.position);
        self.position = self.position.min(self.program.total_frames);
        old
    }

    /// Fills an interleaved stereo output buffer. **Real-time safe**: no
    /// allocation, locks, or I/O.
    pub fn process(&mut self, out: &mut [f32]) {
        let master = self.program.master;
        for frame in out.chunks_exact_mut(2) {
            let limit = if self.looping {
                self.program.song_frames
            } else {
                self.program.total_frames
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

            while self.cursor < self.program.events.len()
                && self.program.events[self.cursor].at == self.position
            {
                let (strip, kind) = {
                    let ev = &self.program.events[self.cursor];
                    (ev.strip, ev.kind)
                };
                self.voices[strip].apply(&kind);
                self.cursor += 1;
            }

            let mut l = 0.0f32;
            let mut r = 0.0f32;
            for (i, voice) in self.voices.iter_mut().enumerate() {
                // Voice -> channel insert chain (mono) -> pan/gain.
                let mut v = voice.tick();
                for insert in &mut self.program.channel_inserts[i] {
                    v = insert.tick(v);
                }
                let (lg, rg) = self.program.gains[i];
                l += v * lg;
                r += v * rg;
            }
            // Mix any audio clips sounding at this frame.
            for clip in &self.program.audio {
                if self.position >= clip.start && self.position < clip.end {
                    let (sl, sr) = clip.data.frames[clip.offset + (self.position - clip.start)];
                    l += sl * clip.lg;
                    r += sr * clip.rg;
                }
            }
            // Master insert chains (per side), then the master fader.
            for insert in &mut self.program.master_l {
                l = insert.tick(l);
            }
            for insert in &mut self.program.master_r {
                r = insert.tick(r);
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
        engine.process(&mut out);
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

    #[test]
    fn live_mute_is_heard_immediately() {
        let mut engine = RealtimeEngine::prepare(&one_kick_project(), None).unwrap();
        engine.play();
        let mut buf = vec![0.0f32; 512 * 2];
        engine.process(&mut buf);
        assert!(
            buf.iter().any(|&s| s != 0.0),
            "kick should be audible first"
        );

        // Mute the channel live.
        let mut muted = one_kick_project();
        muted.channels[0].mixer.mute = true;
        let program = Program::build(&muted, engine.sample_rate()).unwrap();
        let _old = engine.load_program(Box::new(program));

        let mut buf2 = vec![0.0f32; 512 * 2];
        engine.process(&mut buf2);
        assert!(
            buf2.iter().all(|&s| s == 0.0),
            "muting should silence output on the next block"
        );
    }

    #[test]
    fn live_step_edit_is_heard_on_next_loop() {
        // Start from an empty, looping pattern — pure silence.
        let mut empty = one_kick_project();
        empty.patterns[0].drum_lanes[0].steps[0].on = false;
        let mut engine = RealtimeEngine::prepare(&empty, None).unwrap();
        engine.set_looping(true);
        engine.play();

        let mut before = vec![0.0f32; 6_000 * 2];
        engine.process(&mut before);
        assert!(
            before.iter().all(|&s| s == 0.0),
            "empty pattern should be silent"
        );

        // Live-add a kick on step 1 (the project's step 0 is on).
        let program = Program::build(&one_kick_project(), engine.sample_rate()).unwrap();
        let _old = engine.load_program(Box::new(program));

        // Play past the loop boundary (1 bar beat = 24_000 frames @ 120/48k);
        // the newly added hit must fire when the loop wraps to step 0.
        let mut after = vec![0.0f32; 30_000 * 2];
        engine.process(&mut after);
        assert!(
            after.iter().any(|&s| s != 0.0),
            "a step added live should be heard on the next loop pass"
        );
    }

    #[test]
    fn audio_clip_plays_at_its_tick_with_gain() {
        use crate::render::render_offline_with;
        use dhun_project::{AudioClip, AudioTrack, MixerStrip};

        // An impulse clip: frame 0 = 1.0, frame 2 = 0.5.
        let mut assets = AudioAssets::empty();
        let mut frames = vec![(0.0f32, 0.0f32); 16];
        frames[0] = (1.0, 1.0);
        frames[2] = (0.5, 0.5);
        assets.insert_frames("take.wav", frames);

        let mut project = one_kick_project();
        project.patterns[0].drum_lanes[0].steps[0].on = false; // isolate the clip
        project.audio_tracks.push(AudioTrack {
            id: 100,
            name: "Gtr".to_string(),
            mixer: MixerStrip::default(), // 0 dB, center
            clips: vec![AudioClip {
                source: "take.wav".to_string(),
                start_tick: 240, // one step @ 120/48k = 6000 frames
                offset_frames: 0,
                length_frames: 0,
                gain_db: 0.0,
            }],
        });

        let audio = render_offline_with(&project, None, &assets).unwrap();
        let at = 6000usize; // tick_to_sample(240)
                            // Center pan => ~0.707 per side.
        assert!(
            (audio.interleaved[at * 2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3,
            "clip impulse should land at frame {at}"
        );
        assert_eq!(audio.interleaved[(at - 1) * 2], 0.0, "silence just before");
        assert!(
            (audio.interleaved[(at + 2) * 2] - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3,
            "second clip sample two frames later"
        );
    }

    #[test]
    fn channel_insert_gain_is_applied() {
        use crate::render::render_offline;
        use dhun_project::EffectSpec;

        let base = render_offline(&one_kick_project(), None).unwrap();
        let base_peak = base.interleaved.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(base_peak > 0.05, "baseline kick audible");

        // A -120 dB gain insert on the kick channel should silence it.
        let mut p = one_kick_project();
        p.channels[0].mixer.effects = vec![EffectSpec::Gain { gain_db: -120.0 }];
        let cut = render_offline(&p, None).unwrap();
        let cut_peak = cut.interleaved.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            cut_peak < base_peak * 0.01,
            "channel insert not applied: {cut_peak} vs {base_peak}"
        );
    }

    #[test]
    fn master_insert_gain_is_applied() {
        use crate::render::render_offline;
        use dhun_project::EffectSpec;

        let mut p = one_kick_project();
        p.master.effects = vec![EffectSpec::Gain { gain_db: -120.0 }];
        let out = render_offline(&p, None).unwrap();
        let peak = out.interleaved.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1e-3, "master insert should silence output: {peak}");
    }

    #[test]
    fn load_program_rejects_mismatched_channel_count() {
        let mut engine = RealtimeEngine::prepare(&one_kick_project(), None).unwrap();
        assert_eq!(engine.channels(), 1);

        // A two-channel project's program must not be applied to a 1-voice engine.
        let mut two = one_kick_project();
        let mut extra = two.channels[0].clone();
        extra.id = 2;
        two.channels.push(extra);
        let steps = two.patterns[0].drum_lanes[0].steps.clone();
        two.patterns[0]
            .drum_lanes
            .push(dhun_project::DrumLane { channel: 2, steps });

        let program = Program::build(&two, engine.sample_rate()).unwrap();
        let rejected = engine.load_program(Box::new(program));
        assert_eq!(rejected.channels(), 2, "mismatched program handed back");
        assert_eq!(engine.channels(), 1, "engine unchanged");
    }
}
