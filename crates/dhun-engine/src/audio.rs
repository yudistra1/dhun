//! Decoded audio-clip sample data for recorded tracks (SPEC §3.4).
//!
//! Clips are loaded into memory once (off the audio thread) and shared with the
//! engine as `Arc<ClipData>`, so playback on the audio thread is a pure index —
//! no file I/O or allocation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use dhun_project::Project;

use crate::error::EngineError;

/// Decoded clip samples as stereo frames (mono sources are duplicated L/R).
#[derive(Debug)]
pub struct ClipData {
    pub(crate) frames: Vec<(f32, f32)>,
}

impl ClipData {
    /// Number of frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the clip is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// A store of decoded audio clips keyed by their project-relative source path.
#[derive(Default)]
pub struct AudioAssets {
    clips: HashMap<String, Arc<ClipData>>,
}

impl AudioAssets {
    /// An empty store (projects with no audio tracks need nothing).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Inserts decoded stereo frames under a source name (used by tests and
    /// in-memory callers).
    pub fn insert_frames(&mut self, source: impl Into<String>, frames: Vec<(f32, f32)>) {
        self.clips
            .insert(source.into(), Arc::new(ClipData { frames }));
    }

    /// Loads every clip referenced by `project`'s audio tracks from WAV files
    /// under `dir` (each unique source is read once).
    ///
    /// # Errors
    /// Returns [`EngineError::ClipLoad`] if a referenced file cannot be read.
    pub fn load_from_dir(project: &Project, dir: &Path) -> Result<Self, EngineError> {
        let mut assets = Self::empty();
        for track in &project.audio_tracks {
            for clip in &track.clips {
                if assets.clips.contains_key(&clip.source) {
                    continue;
                }
                let frames = read_wav_frames(&dir.join(&clip.source)).map_err(|reason| {
                    EngineError::ClipLoad {
                        path: clip.source.clone(),
                        reason,
                    }
                })?;
                assets.insert_frames(clip.source.clone(), frames);
            }
        }
        Ok(assets)
    }

    /// Looks up a clip's decoded data.
    #[must_use]
    pub(crate) fn get(&self, source: &str) -> Option<Arc<ClipData>> {
        self.clips.get(source).cloned()
    }
}

/// Reads a WAV file into stereo frames in `[-1, 1]`.
fn read_wav_frames(path: &Path) -> Result<Vec<(f32, f32)>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            // Full-scale for the given bit depth: 2^(bits-1).
            let scale = 2.0_f32.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?
        }
    };

    // Interleaved samples -> stereo frames. Mono duplicates to both sides;
    // more than two channels keep the first two.
    let mut frames = Vec::with_capacity(samples.len() / channels);
    for frame in samples.chunks(channels) {
        let l = frame.first().copied().unwrap_or(0.0);
        let r = if channels >= 2 { frame[1] } else { l };
        frames.push((l, r));
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dhun_project::{AudioClip, AudioTrack, MixerStrip, Project};

    #[test]
    fn loads_a_mono_wav_from_dir() {
        // Write a tiny mono 16-bit WAV to a temp dir.
        let dir = std::env::temp_dir().join(format!("dhun_audio_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let path = dir.join("take.wav");
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        w.write_sample(16_384_i16).unwrap(); // ~0.5
        w.write_sample(-16_384_i16).unwrap(); // ~-0.5
        w.finalize().unwrap();

        let project = Project {
            audio_tracks: vec![AudioTrack {
                id: 1,
                name: "Gtr".to_string(),
                mixer: MixerStrip::default(),
                clips: vec![AudioClip {
                    source: "take.wav".to_string(),
                    start_tick: 0,
                    offset_frames: 0,
                    length_frames: 0,
                    gain_db: 0.0,
                }],
            }],
            ..Project::default()
        };

        let assets = AudioAssets::load_from_dir(&project, &dir).unwrap();
        let clip = assets.get("take.wav").expect("clip loaded");
        assert_eq!(clip.len(), 2);
        assert!((clip.frames[0].0 - 0.5).abs() < 1e-3);
        assert!((clip.frames[1].0 + 0.5).abs() < 1e-3);
        // Mono duplicated to both sides.
        assert_eq!(clip.frames[0].0, clip.frames[0].1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_clip_file_errors() {
        let project = Project {
            audio_tracks: vec![AudioTrack {
                id: 1,
                name: "Gtr".to_string(),
                mixer: MixerStrip::default(),
                clips: vec![AudioClip {
                    source: "nope.wav".to_string(),
                    start_tick: 0,
                    offset_frames: 0,
                    length_frames: 0,
                    gain_db: 0.0,
                }],
            }],
            ..Project::default()
        };
        let err = AudioAssets::load_from_dir(&project, std::path::Path::new("/nonexistent"));
        assert!(matches!(err, Err(EngineError::ClipLoad { .. })));
    }
}
