//! Lock-free level meters published by the audio thread for the UI (SPEC §3.5).
//!
//! The engine writes per-channel (post-fader) and master peak levels into
//! atomics each `process` block; the UI thread reads them. Floats are stored as
//! their bit patterns in `AtomicU32`, written/read `Relaxed` — meters are a
//! display aid, so a torn-by-a-block read is harmless.

use std::sync::atomic::{AtomicU32, Ordering};

/// Shared peak levels: one per instrument channel, plus master L/R.
#[derive(Debug)]
pub struct EngineMeters {
    channels: Vec<AtomicU32>,
    master_l: AtomicU32,
    master_r: AtomicU32,
}

impl EngineMeters {
    pub(crate) fn new(channels: usize) -> Self {
        Self {
            channels: (0..channels).map(|_| AtomicU32::new(0)).collect(),
            master_l: AtomicU32::new(0),
            master_r: AtomicU32::new(0),
        }
    }

    /// Number of channel meters.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// The latest post-fader peak for channel `i` (linear, `0.0..`).
    #[must_use]
    pub fn channel_peak(&self, i: usize) -> f32 {
        self.channels
            .get(i)
            .map_or(0.0, |a| f32::from_bits(a.load(Ordering::Relaxed)))
    }

    /// The latest master peak as `(left, right)`.
    #[must_use]
    pub fn master_peak(&self) -> (f32, f32) {
        (
            f32::from_bits(self.master_l.load(Ordering::Relaxed)),
            f32::from_bits(self.master_r.load(Ordering::Relaxed)),
        )
    }

    pub(crate) fn store_channel(&self, i: usize, value: f32) {
        if let Some(a) = self.channels.get(i) {
            a.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn store_master(&self, left: f32, right: f32) {
        self.master_l.store(left.to_bits(), Ordering::Relaxed);
        self.master_r.store(right.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_back() {
        let m = EngineMeters::new(2);
        assert_eq!(m.channel_count(), 2);
        m.store_channel(0, 0.5);
        m.store_channel(1, 0.25);
        m.store_master(0.8, 0.6);
        assert!((m.channel_peak(0) - 0.5).abs() < 1e-6);
        assert!((m.channel_peak(1) - 0.25).abs() < 1e-6);
        assert_eq!(m.channel_peak(99), 0.0);
        let (l, r) = m.master_peak();
        assert!((l - 0.8).abs() < 1e-6 && (r - 0.6).abs() < 1e-6);
    }
}
