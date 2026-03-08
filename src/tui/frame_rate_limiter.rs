//! Frame rate limiting to prevent excessive redraws.
//!
//! When widgets request frequent redraws (e.g., for animations), they may
//! trigger more renders than the user can perceive. This module enforces
//! a maximum frame rate to avoid wasting CPU and battery.
//!
//! The implementation is kept simple and self-contained for easy testing.

use std::time::Duration;
use std::time::Instant;

/// Minimum time between frames at 120 FPS (~8.33ms).
pub(super) const MIN_FRAME_INTERVAL: Duration = Duration::from_nanos(8_333_334);

/// Tracks when the last frame was drawn and enforces rate limits.
#[derive(Debug, Default)]
pub(super) struct FrameRateLimiter {
  last_draw_time: Option<Instant>,
}

impl FrameRateLimiter {
  /// Adjust a requested draw time to respect the frame rate limit.
  ///
  /// If the requested time is too soon after the last draw,
  /// returns a delayed time that satisfies the minimum interval.
  pub(super) fn clamp_deadline(&self, requested: Instant) -> Instant {
    let Some(last_draw) = self.last_draw_time else {
      return requested;
    };
    let earliest_allowed = last_draw
      .checked_add(MIN_FRAME_INTERVAL)
      .unwrap_or(last_draw);
    requested.max(earliest_allowed)
  }

  /// Record that a frame was drawn at the given time.
  pub(super) fn mark_emitted(&mut self, emitted_at: Instant) {
    self.last_draw_time = Some(emitted_at);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_does_not_clamp() {
    let t0 = Instant::now();
    let limiter = FrameRateLimiter::default();
    assert_eq!(limiter.clamp_deadline(t0), t0);
  }

  #[test]
  fn clamps_to_min_interval_since_last_emit() {
    let t0 = Instant::now();
    let mut limiter = FrameRateLimiter::default();

    assert_eq!(limiter.clamp_deadline(t0), t0);
    limiter.mark_emitted(t0);

    let too_soon = t0 + Duration::from_millis(1);
    assert_eq!(limiter.clamp_deadline(too_soon), t0 + MIN_FRAME_INTERVAL);
  }
}
