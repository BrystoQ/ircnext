//! Send rate limiting (token bucket), to avoid being disconnected by the
//! server for “Excess Flood” (when pasting many lines, for example).

use std::time::Duration;

use tokio::time::Instant;

/// Token bucket: `burst` messages in a row, then one every `interval`.
#[derive(Debug, Clone)]
pub struct Throttle {
    burst: u32,
    interval: Duration,
    tokens: u32,
    /// Time of the last token added.
    refilled: Instant,
}

impl Throttle {
    pub fn new(burst: u32, interval: Duration, now: Instant) -> Self {
        Self { burst, interval, tokens: burst, refilled: now }
    }

    /// Takes a token if there is one.
    pub fn try_acquire(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }

    /// Time when the next token will be available.
    pub fn ready_at(&self) -> Instant {
        self.refilled + self.interval
    }

    fn refill(&mut self, now: Instant) {
        if self.tokens == self.burst {
            self.refilled = now;
            return;
        }
        let elapsed = now.saturating_duration_since(self.refilled);
        let gained = u32::try_from(elapsed.as_nanos() / self.interval.as_nanos().max(1)).unwrap_or(u32::MAX);
        if gained > 0 {
            self.tokens = self.tokens.saturating_add(gained).min(self.burst);
            // Full bucket: time elapsed beyond that no longer counts.
            self.refilled = if self.tokens == self.burst { now } else { self.refilled + self.interval * gained };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_a_burst_then_one_message_per_interval() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut throttle = Throttle::new(3, second, start);
        assert!((0..3).all(|_| throttle.try_acquire(start)));
        assert!(!throttle.try_acquire(start));
        assert_eq!(throttle.ready_at(), start + second);

        assert!(!throttle.try_acquire(start + second / 2));
        assert!(throttle.try_acquire(start + second));
        assert!(!throttle.try_acquire(start + second));
    }

    #[test]
    fn refills_up_to_the_burst_size() {
        let start = Instant::now();
        let mut throttle = Throttle::new(2, Duration::from_secs(1), start);
        assert!(throttle.try_acquire(start) && throttle.try_acquire(start));
        let later = start + Duration::from_secs(60);
        assert!(throttle.try_acquire(later) && throttle.try_acquire(later));
        assert!(!throttle.try_acquire(later));
    }
}
