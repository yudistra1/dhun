//! Real-time audio input capture to a WAV file (SPEC §3.4).
//!
//! The input callback pushes samples into a lock-free ring (no allocation or
//! locks on the audio thread); a background writer thread drains the ring and
//! writes a 32-bit float WAV. The captured take can then be referenced by an
//! [`dhun_project::AudioClip`] and played back by the engine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::IoError;

/// Summary of a finished recording.
#[derive(Debug, Clone)]
pub struct Recording {
    /// Where the WAV was written.
    pub path: PathBuf,
    /// Number of frames captured.
    pub frames: u64,
    /// Channels captured.
    pub channels: u16,
    /// Capture sample rate in hertz.
    pub sample_rate: u32,
}

/// A live recording session capturing the default input device to a WAV file.
pub struct Recorder {
    _stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    writer: Option<JoinHandle<Result<u64, IoError>>>,
    path: PathBuf,
    channels: u16,
    sample_rate: u32,
}

impl Recorder {
    /// Opens the default input device and starts capturing to `path`.
    ///
    /// Recording runs until [`Recorder::stop`] is called.
    ///
    /// # Errors
    /// Returns an [`IoError`] if there is no input device, its format is
    /// unsupported, or the stream/WAV writer cannot be created.
    pub fn start(path: impl AsRef<Path>) -> Result<Self, IoError> {
        let path = path.as_ref().to_path_buf();
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(IoError::NoInputDevice)?;
        let supported = device.default_input_config()?;
        let sample_format = supported.sample_format();
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let config = supported.config();

        // ~4 seconds of headroom so the writer never has to keep up sample-exact.
        let capacity = (sample_rate as usize * channels as usize * 4).max(4096);
        let (producer, consumer) = RingBuffer::<f32>::new(capacity);

        let stop = Arc::new(AtomicBool::new(false));
        let writer = spawn_writer(
            path.clone(),
            channels,
            sample_rate,
            consumer,
            Arc::clone(&stop),
        )?;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_input::<f32>(&device, &config, producer)?,
            cpal::SampleFormat::I16 => build_input::<i16>(&device, &config, producer)?,
            cpal::SampleFormat::U16 => build_input::<u16>(&device, &config, producer)?,
            other => return Err(IoError::UnsupportedFormat(other)),
        };
        stream.play()?;

        Ok(Self {
            _stream: stream,
            stop,
            writer: Some(writer),
            path,
            channels,
            sample_rate,
        })
    }

    /// The capture sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Stops capture, finalizes the WAV, and returns a [`Recording`] summary.
    ///
    /// # Errors
    /// Returns an [`IoError`] if the writer thread failed.
    pub fn stop(mut self) -> Result<Recording, IoError> {
        self.stop.store(true, Ordering::Relaxed);
        let frames = match self.writer.take() {
            Some(handle) => handle.join().map_err(|_| IoError::WriterPanicked)??,
            None => 0,
        };
        Ok(Recording {
            path: self.path.clone(),
            frames,
            channels: self.channels,
            sample_rate: self.sample_rate,
        })
    }
}

/// Spawns the background thread that drains captured samples to a float WAV.
fn spawn_writer(
    path: PathBuf,
    channels: u16,
    sample_rate: u32,
    mut consumer: Consumer<f32>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<Result<u64, IoError>>, IoError> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&path, spec)?;

    let handle = std::thread::spawn(move || -> Result<u64, IoError> {
        let mut samples: u64 = 0;
        loop {
            let mut drained = false;
            while let Ok(sample) = consumer.pop() {
                writer.write_sample(sample)?;
                samples += 1;
                drained = true;
            }
            if stop.load(Ordering::Relaxed) && !drained {
                break;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        writer.finalize()?;
        Ok(samples / u64::from(channels.max(1)))
    });
    Ok(handle)
}

/// Builds the input stream for a concrete sample format, pushing captured
/// samples (converted to `f32`) into the ring.
fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: Producer<f32>,
) -> Result<cpal::Stream, IoError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                // If the writer falls behind and the ring is full, drop the
                // sample rather than block the audio thread.
                let _ = producer.push(f32::from_sample(sample));
            }
        },
        move |err| eprintln!("dhun-io input error: {err}"),
        None,
    )?;
    Ok(stream)
}
