//! `dhun-io` — real-time audio device output via `cpal` (SPEC §6).
//!
//! [`Playback`] opens the default output device and drives a
//! [`dhun_engine::RealtimeEngine`] from the audio callback. Transport commands
//! cross to the audio thread over an `rtrb` lock-free ring buffer, so the main
//! thread never blocks the callback and the callback never locks (CLAUDE.md
//! real-time rules).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample, StreamConfig};
use dhun_engine::{Project, RealtimeEngine};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;

/// Errors from opening or driving the audio device.
#[derive(Debug, Error)]
pub enum IoError {
    /// No default output device is available.
    #[error("no default audio output device")]
    NoOutputDevice,

    /// The device uses a sample format Dhun does not handle.
    #[error("unsupported output sample format: {0:?}")]
    UnsupportedFormat(cpal::SampleFormat),

    /// Preparing the engine for the device failed.
    #[error(transparent)]
    Engine(#[from] dhun_engine::EngineError),

    /// Querying the default output config failed.
    #[error("default output config error: {0}")]
    Config(#[from] cpal::DefaultStreamConfigError),

    /// Building the output stream failed.
    #[error("build stream error: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),

    /// Starting the output stream failed.
    #[error("play stream error: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
}

/// A transport command sent to the audio thread.
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// Start or resume playback.
    Play,
    /// Pause playback.
    Stop,
    /// Return the playhead to the start.
    Rewind,
    /// Enable or disable looping.
    SetLooping(bool),
}

/// A live audio-output session: owns the device stream and the command channel.
pub struct Playback {
    _stream: cpal::Stream,
    commands: Producer<Command>,
    finished: Arc<AtomicBool>,
    sample_rate: u32,
}

impl Playback {
    /// Opens the default output device and starts streaming the project's
    /// real-time render. Playback begins paused; call [`Playback::play`].
    ///
    /// The engine is prepared at the device's own sample rate.
    ///
    /// # Errors
    /// Returns an [`IoError`] if there is no output device, the device's
    /// sample rate or format is unsupported, or the stream cannot be built or
    /// started.
    pub fn start(project: &Project) -> Result<Self, IoError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(IoError::NoOutputDevice)?;
        let supported = device.default_output_config()?;
        let sample_format = supported.sample_format();
        let sample_rate = supported.sample_rate().0;
        let config: StreamConfig = supported.config();

        let engine = RealtimeEngine::prepare(project, Some(sample_rate))?;
        let (producer, consumer) = RingBuffer::<Command>::new(64);
        let finished = Arc::new(AtomicBool::new(false));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(&device, &config, engine, consumer, Arc::clone(&finished))?
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(&device, &config, engine, consumer, Arc::clone(&finished))?
            }
            cpal::SampleFormat::U16 => {
                build_stream::<u16>(&device, &config, engine, consumer, Arc::clone(&finished))?
            }
            other => return Err(IoError::UnsupportedFormat(other)),
        };
        stream.play()?;

        Ok(Self {
            _stream: stream,
            commands: producer,
            finished,
            sample_rate,
        })
    }

    /// The device sample rate playback is running at.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether non-looping playback has reached the end.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    /// Starts or resumes playback.
    pub fn play(&mut self) {
        self.send(Command::Play);
    }

    /// Pauses playback.
    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }

    /// Returns the playhead to the start.
    pub fn rewind(&mut self) {
        self.send(Command::Rewind);
    }

    /// Enables or disables looping.
    pub fn set_looping(&mut self, looping: bool) {
        self.send(Command::SetLooping(looping));
    }

    /// Queues a command for the audio thread (dropped if the queue is full,
    /// which only happens under pathological command spam).
    fn send(&mut self, command: Command) {
        let _ = self.commands.push(command);
    }
}

/// Builds the output stream for a concrete sample format.
fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut engine: RealtimeEngine,
    mut commands: Consumer<Command>,
    finished: Arc<AtomicBool>,
) -> Result<cpal::Stream, IoError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // Drain transport commands (lock-free, allocation-free).
            while let Ok(command) = commands.pop() {
                apply(&mut engine, command);
            }
            // Render one stereo frame at a time into whatever channel layout
            // the device uses. `frame` lives on the stack — no allocation.
            let mut frame = [0.0f32; 2];
            for out in data.chunks_mut(channels) {
                engine.process(&mut frame);
                for (i, slot) in out.iter_mut().enumerate() {
                    let v = if i < 2 { frame[i] } else { 0.0 };
                    *slot = T::from_sample(v);
                }
            }
            finished.store(engine.is_finished(), Ordering::Relaxed);
        },
        move |err| eprintln!("dhun-io stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

fn apply(engine: &mut RealtimeEngine, command: Command) {
    match command {
        Command::Play => engine.play(),
        Command::Stop => engine.stop(),
        Command::Rewind => engine.rewind(),
        Command::SetLooping(on) => engine.set_looping(on),
    }
}
