//! Detects dense output in a short window without accumulating sparse traffic.
use std::time::{Duration, Instant};

pub(super) const TRIGGER: usize = 64 << 10;
pub(super) const WINDOW: Duration = Duration::from_millis(2);

#[derive(Default)]
pub(super) struct Burst {
	start: Option<Instant>,
	bytes: usize,
	until: Option<Instant>,
}

impl Burst {
	pub(super) fn received(&mut self, bytes: usize, now: Instant) {
		if bytes == 0 {
			return;
		}
		if self
			.start
			.is_none_or(|start| now.saturating_duration_since(start) >= WINDOW)
		{
			self.start = Some(now);
			self.bytes = 0;
		}
		self.bytes = self.bytes.saturating_add(bytes);
		if self.bytes >= TRIGGER {
			self.until = Some(now + WINDOW);
		}
	}

	pub(super) fn active(&self, now: Instant) -> bool {
		self.until.is_some_and(|until| now < until)
	}
}

#[cfg(test)]
#[path = "burst_tests.rs"]
mod tests;
