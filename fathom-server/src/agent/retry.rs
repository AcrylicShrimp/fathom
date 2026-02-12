use std::time::Duration;

use crate::util::now_unix_ms;

#[derive(Debug, Clone)]
pub(crate) struct RetryPolicy {
    max_retries: usize,
    base_delay_ms: u64,
    max_delay_ms: u64,
    jitter_ms: u64,
}

impl RetryPolicy {
    pub(crate) fn conservative() -> Self {
        Self {
            max_retries: 2,
            base_delay_ms: 400,
            max_delay_ms: 4_000,
            jitter_ms: 300,
        }
    }

    pub(crate) fn max_retries(&self) -> usize {
        self.max_retries
    }

    pub(crate) fn compute_delay(&self, attempt: usize, retry_after: Option<Duration>) -> Duration {
        if let Some(retry_after) = retry_after {
            return retry_after;
        }

        let exp = 2u64
            .saturating_pow(attempt as u32)
            .saturating_mul(self.base_delay_ms);
        let bounded = exp.min(self.max_delay_ms);
        let jitter = if self.jitter_ms == 0 {
            0
        } else {
            (now_unix_ms().unsigned_abs() % self.jitter_ms) as u64
        };

        Duration::from_millis(bounded.saturating_add(jitter))
    }
}
