//! Per-adapter dynamic state.
//!
//! [`State`] holds the values exposed by [`Progressive`] for a single adapter (message, progress,
//! cumulative bytes, total, EWMA rate, elapsed-time start). Rendering, cursor management and
//! spinner ticking live elsewhere ([`Line`](crate::line::Line) and the wrapper itself); `State`
//! only stores and updates the numbers.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use crate::progressive::Progressive;

/// Minimum interval between rate samples. Resampling more often than this magnifies tiny per-frame
/// timing jitter into wild speed numbers; waiting at least this long lets a meaningful number of
/// bytes accumulate.
const RATE_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

/// Smoothing factor for the EWMA rate. Higher values track recent throughput more aggressively;
/// lower values produce a steadier display at the cost of lag.
const RATE_ALPHA: f64 = 0.3;

pub(crate) struct State {
    pub(crate) label: Option<String>,
    pub(crate) message: Option<Cow<'static, str>>,
    pub(crate) progress: Option<f64>,
    pub(crate) bytes_done: u64,
    pub(crate) bytes_total: Option<u64>,
    pub(crate) rate: Option<f64>,
    /// Last `(time, bytes_done)` sample used to derive the EWMA rate. `None` until the first
    /// [`add_bytes`](Self::add_bytes) call.
    rate_sample: Option<(Instant, u64)>,
    pub(crate) with_elapsed_time: bool,
    pub(crate) start: Option<Instant>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            label: None,
            message: None,
            progress: None,
            bytes_done: 0,
            bytes_total: None,
            rate: None,
            rate_sample: None,
            with_elapsed_time: false,
            start: None,
        }
    }

    pub(crate) fn set_label(&mut self, label: String) {
        self.label = Some(label);
    }

    pub(crate) fn set_message(&mut self, message: Cow<'static, str>) {
        self.message = Some(message);
    }

    pub(crate) fn set_progress(&mut self, progress: f64) {
        self.progress = Some(progress);
    }

    pub(crate) fn set_bytes_total(&mut self, total: u64) {
        self.bytes_total = Some(total);
    }

    /// Record `delta` newly-transferred bytes. Updates the cumulative counter, the EWMA rate
    /// (resampled at most every [`RATE_SAMPLE_INTERVAL`]) and the progress fraction when a total
    /// is known.
    pub(crate) fn add_bytes(&mut self, delta: u64) {
        self.bytes_done = self.bytes_done.saturating_add(delta);
        let now = Instant::now();

        match self.rate_sample {
            None => self.rate_sample = Some((now, self.bytes_done)),
            Some((last_time, last_bytes)) => {
                let dt = now.duration_since(last_time);
                if dt >= RATE_SAMPLE_INTERVAL {
                    let bytes_delta = self.bytes_done.saturating_sub(last_bytes) as f64;
                    let instantaneous = bytes_delta / dt.as_secs_f64();
                    self.rate = Some(match self.rate {
                        None => instantaneous,
                        Some(r) => RATE_ALPHA * instantaneous + (1.0 - RATE_ALPHA) * r,
                    });
                    self.rate_sample = Some((now, self.bytes_done));
                }
            }
        }

        if let Some(total) = self.bytes_total {
            if total > 0 {
                self.progress = Some(self.bytes_done as f64 / total as f64);
            }
        }
    }

    pub(crate) fn enable_elapsed_time(&mut self) {
        self.with_elapsed_time = true;
    }

    /// Elapsed time since the first call to this method. Lazily captures the start instant so the
    /// reported duration matches "since first frame", not "since builder construction".
    pub(crate) fn elapsed(&mut self) -> Duration {
        self.start.get_or_insert_with(Instant::now).elapsed()
    }
}

impl Progressive for State {
    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn progress(&self) -> Option<f64> {
        self.progress
    }

    fn bytes_done(&self) -> u64 {
        self.bytes_done
    }

    fn bytes_total(&self) -> Option<u64> {
        self.bytes_total
    }

    fn rate(&self) -> Option<f64> {
        self.rate
    }
}
