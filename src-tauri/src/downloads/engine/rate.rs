use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{sync::Mutex, time};
use tokio_util::sync::CancellationToken;

use super::EngineError;

const SPEED_SMOOTHING_WINDOW: Duration = Duration::from_millis(2_500);
const SPEED_STALL_GRACE: Duration = Duration::from_millis(1_200);
const SPEED_STALL_TIMEOUT: Duration = Duration::from_secs(4);

pub(super) struct TransferRateEstimator {
    checkpoint_bytes: u64,
    checkpoint_at: Instant,
    smoothed_bytes_per_second: f64,
    last_progress_at: Option<Instant>,
}

impl TransferRateEstimator {
    pub(super) fn new(downloaded_bytes: u64, now: Instant) -> Self {
        Self {
            checkpoint_bytes: downloaded_bytes,
            checkpoint_at: now,
            smoothed_bytes_per_second: 0.0,
            last_progress_at: None,
        }
    }

    pub(super) fn sample(&mut self, downloaded_bytes: u64, now: Instant) -> u64 {
        let elapsed = now.saturating_duration_since(self.checkpoint_at);
        if elapsed.is_zero() {
            return self.current();
        }

        let transferred = downloaded_bytes.saturating_sub(self.checkpoint_bytes);
        self.checkpoint_bytes = downloaded_bytes;
        self.checkpoint_at = now;

        if transferred > 0 {
            let observed = bytes_per_second(transferred, elapsed) as f64;
            if self.smoothed_bytes_per_second <= 0.0 {
                self.smoothed_bytes_per_second = observed;
            } else {
                let alpha = smoothing_alpha(elapsed);
                self.smoothed_bytes_per_second +=
                    alpha * (observed - self.smoothed_bytes_per_second);
            }
            self.last_progress_at = Some(now);
        } else if let Some(last_progress_at) = self.last_progress_at {
            let stalled_for = now.saturating_duration_since(last_progress_at);
            if stalled_for >= SPEED_STALL_TIMEOUT {
                self.smoothed_bytes_per_second = 0.0;
            } else if stalled_for > SPEED_STALL_GRACE {
                self.smoothed_bytes_per_second *= 1.0 - smoothing_alpha(elapsed);
            }
        }

        self.current()
    }

    fn current(&self) -> u64 {
        self.smoothed_bytes_per_second.max(0.0).round() as u64
    }
}

fn smoothing_alpha(elapsed: Duration) -> f64 {
    1.0 - (-elapsed.as_secs_f64() / SPEED_SMOOTHING_WINDOW.as_secs_f64()).exp()
}

#[derive(Clone)]
pub(super) struct BandwidthLimiter {
    bytes_per_second: u64,
    next_available: Arc<Mutex<Instant>>,
}

impl BandwidthLimiter {
    pub(super) fn new(bytes_per_second: u64) -> Self {
        Self {
            bytes_per_second,
            next_available: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub(super) async fn acquire(
        &self,
        bytes: usize,
        cancellation: &CancellationToken,
    ) -> Result<(), EngineError> {
        if self.bytes_per_second == 0 || bytes == 0 {
            return Ok(());
        }
        let wait = {
            let mut next_available = self.next_available.lock().await;
            let now = Instant::now();
            if *next_available < now {
                *next_available = now;
            }
            *next_available += transfer_duration(bytes, self.bytes_per_second);
            next_available.saturating_duration_since(now)
        };
        if wait.is_zero() {
            return Ok(());
        }
        tokio::select! {
            _ = cancellation.cancelled() => Err(EngineError::Cancelled),
            _ = time::sleep(wait) => Ok(()),
        }
    }
}

pub(super) fn bytes_per_second(bytes: u64, elapsed: Duration) -> u64 {
    if elapsed.is_zero() {
        return 0;
    }
    (bytes as f64 / elapsed.as_secs_f64()) as u64
}

pub(super) fn transfer_duration(bytes: usize, bytes_per_second: u64) -> Duration {
    Duration::from_secs_f64(bytes as f64 / bytes_per_second as f64)
}
