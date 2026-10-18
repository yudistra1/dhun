//! Offline, deterministic render of a [`Project`] to interleaved stereo audio
//! (SPEC §3.6, §5 "Determinism").
//!
//! This path runs off the audio thread, so it may allocate freely; the DSP it
//! drives is the same allocation-free code used in real time.

use std::f32::consts::FRAC_PI_2;

use dhun_core::SampleRate;
use dhun_dsp::db_to_lin;
use dhun_project::Project;

use crate::error::EngineError;
use crate::schedule::{collect_channel_events, song_total_samples, ScheduledEvent, Timing};
use crate::voice::{build_voice, Voice};

/// Extra time rendered after the song ends so note/drum tails are not cut off.
pub(crate) const TAIL_SECONDS: f64 = 2.0;

/// A render range in whole bars (4/4). End is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderRange {
    /// First bar to include (0-based).
    pub start_bar: u32,
    /// One past the last bar to include.
    pub end_bar: u32,
}

/// The result of an offline render: interleaved stereo `f32` in `[-1, 1]`.
#[derive(Debug, Clone)]
pub struct RenderedAudio {
    /// Sample rate in hertz.
    pub sample_rate: u32,
    /// Number of stereo frames.
    pub frames: usize,
    /// Interleaved L/R samples, length `frames * 2`.
    pub interleaved: Vec<f32>,
}

impl RenderedAudio {
    /// Number of audio channels (always stereo in v0.1).
    pub const CHANNELS: u16 = 2;

    /// Converts the float samples to 16-bit PCM, clamping to `[-1, 1]`.
    ///
    /// Centralized here so the CLI's WAV output and the golden test quantize
    /// identically.
    #[must_use]
    pub fn to_i16(&self) -> Vec<i16> {
        self.interleaved
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
            .collect()
    }
}

/// Precomputed left/right mix gains for a channel's pan + volume.
pub(crate) fn pan_gains(gain_db: f32, pan: f32) -> (f32, f32) {
    let gain = db_to_lin(gain_db);
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * FRAC_PI_2;
    (theta.cos() * gain, theta.sin() * gain)
}

/// Renders a validated project offline.
///
/// `range` limits output to a bar window; `None` renders the whole song plus a
/// short tail.
///
/// # Errors
/// Returns [`EngineError::Project`] if the project fails validation and
/// [`EngineError::BadRange`] if `range` is empty or inverted.
pub fn render_offline(
    project: &Project,
    range: Option<RenderRange>,
) -> Result<RenderedAudio, EngineError> {
    project.validate()?;

    if let Some(r) = range {
        if r.end_bar <= r.start_bar {
            return Err(EngineError::BadRange {
                start: r.start_bar,
                end: r.end_bar,
            });
        }
    }

    let sample_rate = SampleRate::new(project.sample_rate).expect("sample rate validated");
    let timing = Timing::new(project, sample_rate);
    let tail = (TAIL_SECONDS * f64::from(timing.sample_rate())) as usize;
    let total_frames = song_total_samples(project, &timing) + tail;

    let mut left = vec![0.0f32; total_frames];
    let mut right = vec![0.0f32; total_frames];
    let any_solo = project.channels.iter().any(|c| c.mixer.solo);

    for channel in &project.channels {
        if channel.mixer.mute || (any_solo && !channel.mixer.solo) {
            continue;
        }

        let mut voice = build_voice(&channel.instrument, timing.sample_rate() as f32);
        let mut events = collect_channel_events(project, channel.id, &timing);
        events.retain(|e| e.at < total_frames);
        events.sort_by_key(|e| e.at);

        let mono = render_voice(&mut voice, &events, total_frames);
        let (lg, rg) = pan_gains(channel.mixer.gain_db, channel.mixer.pan);
        for (i, &s) in mono.iter().enumerate() {
            left[i] += s * lg;
            right[i] += s * rg;
        }
    }

    let master = db_to_lin(project.master.gain_db);
    let mut interleaved = Vec::with_capacity(total_frames * 2);
    for i in 0..total_frames {
        interleaved.push(left[i] * master);
        interleaved.push(right[i] * master);
    }

    let mut audio = RenderedAudio {
        sample_rate: timing.sample_rate(),
        frames: total_frames,
        interleaved,
    };

    if let Some(r) = range {
        let start = timing.bar_to_frame(r.start_bar).min(audio.frames);
        let end = timing.bar_to_frame(r.end_bar).min(audio.frames);
        audio.interleaved = audio.interleaved[start * 2..end * 2].to_vec();
        audio.frames = end - start;
    }

    Ok(audio)
}

/// Renders one voice to a mono buffer, applying its events in time order.
fn render_voice(voice: &mut Voice, events: &[ScheduledEvent], total_frames: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; total_frames];
    let mut ei = 0;
    for (i, out) in buf.iter_mut().enumerate() {
        while ei < events.len() && events[ei].at == i {
            voice.apply(&events[ei].kind);
            ei += 1;
        }
        *out = voice.tick();
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::one_kick_project;

    #[test]
    fn renders_stereo_and_finite() {
        let audio = render_offline(&one_kick_project(), None).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.interleaved.len(), audio.frames * 2);
        assert!(audio.interleaved.iter().all(|s| s.is_finite()));
        assert!(audio.interleaved.iter().all(|s| s.abs() <= 1.5));
    }

    #[test]
    fn kick_on_beat_one_produces_early_energy() {
        let audio = render_offline(&one_kick_project(), None).unwrap();
        let early_peak = audio.interleaved[..960 * 2]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(early_peak > 0.05, "kick onset too quiet: {early_peak}");
    }

    #[test]
    fn render_is_deterministic() {
        let p = one_kick_project();
        let a = render_offline(&p, None).unwrap();
        let b = render_offline(&p, None).unwrap();
        assert_eq!(a.interleaved, b.interleaved, "render not deterministic");
    }

    #[test]
    fn all_steps_off_is_silent() {
        let mut p = one_kick_project();
        p.patterns[0].drum_lanes[0]
            .steps
            .iter_mut()
            .for_each(|s| s.on = false);
        let audio = render_offline(&p, None).unwrap();
        assert!(
            audio.interleaved.iter().all(|&s| s == 0.0),
            "expected silence"
        );
    }

    #[test]
    fn muted_channel_is_silent() {
        let mut p = one_kick_project();
        p.channels[0].mixer.mute = true;
        let audio = render_offline(&p, None).unwrap();
        assert!(audio.interleaved.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn to_i16_clamps_and_scales() {
        let audio = RenderedAudio {
            sample_rate: 48_000,
            frames: 3,
            interleaved: vec![0.0, 1.0, -1.0, 2.0, -2.0, 0.5],
        };
        let pcm = audio.to_i16();
        assert_eq!(pcm[0], 0);
        assert_eq!(pcm[1], i16::MAX);
        assert_eq!(pcm[2], -i16::MAX);
        assert_eq!(pcm[3], i16::MAX, "clamps above 1.0");
        assert_eq!(pcm[4], -i16::MAX, "clamps below -1.0");
    }
}
