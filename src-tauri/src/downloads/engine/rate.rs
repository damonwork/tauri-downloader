use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
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

struct RateBudget {
    updated_at: Instant,
    available: f64,
}

#[derive(Clone)]
pub(super) struct BandwidthLimiter {
    bytes_per_second: Arc<AtomicU64>,
    budget: Arc<Mutex<RateBudget>>,
}

impl BandwidthLimiter {
    pub(super) fn shared(bytes_per_second: Arc<AtomicU64>) -> Self {
        Self {
            bytes_per_second,
            budget: Arc::new(Mutex::new(RateBudget {
                updated_at: Instant::now(),
                available: 0.0,
            })),
        }
    }

    pub(super) async fn acquire(
        &self,
        bytes: usize,
        cancellation: &CancellationToken,
    ) -> Result<(), EngineError> {
        let mut remaining = bytes;
        while remaining > 0 {
            if cancellation.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            let rate = self.bytes_per_second.load(Ordering::Relaxed);
            if rate == 0 {
                let mut budget = self.budget.lock().await;
                budget.available = 0.0;
                budget.updated_at = Instant::now();
                return Ok(());
            }
            let wait = {
                let mut budget = self.budget.lock().await;
                let now = Instant::now();
                let capacity = (rate as f64 * 0.1).max(1.0);
                budget.available = (budget.available
                    + now.duration_since(budget.updated_at).as_secs_f64() * rate as f64)
                    .min(capacity);
                budget.updated_at = now;
                let granted = (budget.available.floor() as usize).min(remaining);
                budget.available -= granted as f64;
                remaining -= granted;
                Duration::from_secs_f64((remaining as f64 / rate as f64).clamp(0.001, 0.1))
            };
            if remaining == 0 {
                return Ok(());
            }
            // Poll the live limit while waiting: raising/removing it must not
            // leave a chunk asleep for minutes under the previous setting.
            tokio::select! {
                _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
                _ = time::sleep(wait) => {}
            }
        }
        Ok(())
    }
}

pub(super) fn bytes_per_second(bytes: u64, elapsed: Duration) -> u64 {
    if elapsed.is_zero() {
        return 0;
    }
    (bytes as f64 / elapsed.as_secs_f64()) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cloned_limiters_share_one_budget() {
        let limiter = BandwidthLimiter::shared(Arc::new(AtomicU64::new(128 * 1024)));
        let second = limiter.clone();
        let token = CancellationToken::new();
        let started = Instant::now();
        let (a, b) = tokio::join!(
            limiter.acquire(64 * 1024, &token),
            second.acquire(64 * 1024, &token)
        );
        a.unwrap();
        b.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(900));
    }
}
