//! Bytes read from a PTY, waiting for the parser thread.
//!
//! The reader thread keeps reading while the parser thread parses, so the
//! kernel's PTY buffer keeps draining. When parsing falls behind by `limit`
//! bytes the reader waits, which pushes back on the program producing
//! output instead of buffering without bound.
#[cfg(any(target_os = "macos", test))]
use super::burst::Burst;
use std::sync::{Condvar, Mutex, MutexGuard};
#[cfg(any(target_os = "macos", test))]
use std::time::{Duration, Instant};

pub(crate) struct OutputQueue {
	state: Mutex<State>,
	space: Condvar,
	data: Condvar,
	limit: usize,
	#[cfg(any(target_os = "macos", test))]
	batch_wait: Duration,
	#[cfg(any(target_os = "macos", test))]
	batch_target: usize,
	#[cfg(test)]
	coalescing: std::sync::atomic::AtomicBool,
	/// Notifications sent: (data, space).
	#[cfg(test)]
	wakes: [std::sync::atomic::AtomicUsize; 2],
}

#[derive(Default)]
struct State {
	bytes: Vec<u8>,
	closed: bool,
	#[cfg(any(target_os = "macos", test))]
	burst: Burst,
}

impl OutputQueue {
	pub(crate) fn new(limit: usize) -> Self {
		Self {
			state: Mutex::default(),
			space: Condvar::new(),
			data: Condvar::new(),
			limit,
			#[cfg(any(target_os = "macos", test))]
			batch_wait: if cfg!(target_os = "macos") {
				Duration::from_micros(50)
			} else {
				Duration::ZERO
			},
			#[cfg(any(target_os = "macos", test))]
			batch_target: limit.clamp(1, 4 << 10),
			#[cfg(test)]
			coalescing: std::sync::atomic::AtomicBool::new(false),
			#[cfg(test)]
			wakes: Default::default(),
		}
	}

	/// Bytes the queue holds before the reader waits.
	pub(crate) fn limit(&self) -> usize {
		self.limit
	}

	/// The parser's batch target; none where it does not batch.
	fn batch_target(&self) -> usize {
		#[cfg(any(target_os = "macos", test))]
		return self.batch_target;
		#[cfg(not(any(target_os = "macos", test)))]
		usize::MAX
	}

	/// Wakes the parser, waiting for bytes.
	fn wake_parser(&self) {
		#[cfg(test)]
		self.wakes[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		self.data.notify_one();
	}

	/// Wakes the reader, waiting for space.
	fn wake_reader(&self) {
		#[cfg(test)]
		self.wakes[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		self.space.notify_all();
	}

	/// The notifications sent so far: (data, space).
	#[cfg(test)]
	fn wakes(&self) -> (usize, usize) {
		let [data, space] = self
			.wakes
			.each_ref()
			.map(|n| n.load(std::sync::atomic::Ordering::Relaxed));
		(data, space)
	}

	fn lock(&self) -> MutexGuard<'_, State> {
		self
			.state
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	/// Appends `data`, waiting while the queue is full.
	pub(crate) fn push(&self, data: &[u8]) {
		let mut state = self.lock();
		while state.bytes.len() >= self.limit && !state.closed {
			state = self
				.space
				.wait(state)
				.unwrap_or_else(|poisoned| poisoned.into_inner());
		}
		let before = state.bytes.len();
		state.bytes.extend_from_slice(data);
		#[cfg(any(target_os = "macos", test))]
		state.burst.received(data.len(), Instant::now());
		if wakes_parser(before, state.bytes.len(), self.batch_target()) {
			self.wake_parser();
		}
	}

	/// Waits for queued bytes and moves them into `out`. Returns false once
	/// the queue is closed and fully drained.
	pub(crate) fn wait_take(&self, out: &mut Vec<u8>) -> bool {
		out.clear();
		let mut state = self.lock();
		while state.bytes.is_empty() && !state.closed {
			state = self
				.data
				.wait(state)
				.unwrap_or_else(|poisoned| poisoned.into_inner());
		}
		if state.bytes.is_empty() {
			return false;
		}
		#[cfg(any(target_os = "macos", test))]
		if !state.closed
			&& state.bytes.len() < self.batch_target
			&& !self.batch_wait.is_zero()
			&& state.burst.active(Instant::now())
		{
			#[cfg(test)]
			self
				.coalescing
				.store(true, std::sync::atomic::Ordering::Release);
			let (next, _) = self
				.data
				.wait_timeout_while(state, self.batch_wait, |state| {
					!state.closed && state.bytes.len() < self.batch_target
				})
				.unwrap_or_else(|poisoned| poisoned.into_inner());
			state = next;
			#[cfg(test)]
			self
				.coalescing
				.store(false, std::sync::atomic::Ordering::Release);
		}
		// The reader waits only while the queue is full.
		let full = state.bytes.len() >= self.limit;
		// Swapping keeps both buffers' capacity.
		std::mem::swap(out, &mut state.bytes);
		if full {
			self.wake_reader();
		}
		true
	}

	/// Ends the stream: wakes a reader blocked in [`push`](Self::push) (e.g.
	/// when the tab closes) and lets the parser finish once drained.
	pub(crate) fn close(&self) {
		self.lock().closed = true;
		self.space.notify_all();
		self.data.notify_all();
	}
}

/// Whether a push that took the queue from `before` to `after` bytes must
/// wake the parser, which sleeps only on an empty queue or, batching, below
/// `batch_target` bytes.
fn wakes_parser(before: usize, after: usize, batch_target: usize) -> bool {
	after > before && (before == 0 || (before < batch_target && after >= batch_target))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{sync::Arc, thread, time::Duration};

	#[test]
	fn a_full_queue_blocks_the_reader_until_the_parser_takes_bytes() {
		let queue = Arc::new(OutputQueue::new(4));
		queue.push(b"1234");
		let reader = {
			let queue = queue.clone();
			thread::spawn(move || queue.push(b"5"))
		};
		thread::sleep(Duration::from_millis(50));
		assert!(
			!reader.is_finished(),
			"push must wait while the queue is full"
		);
		let mut out = Vec::new();
		assert!(queue.wait_take(&mut out));
		reader.join().unwrap();
		assert!(queue.wait_take(&mut out));
		assert_eq!(out, b"5");
	}

	#[test]
	fn closing_releases_a_blocked_reader() {
		let queue = Arc::new(OutputQueue::new(1));
		queue.push(b"x");
		let reader = {
			let queue = queue.clone();
			thread::spawn(move || queue.push(b"y"))
		};
		thread::sleep(Duration::from_millis(20));
		queue.close();
		reader.join().unwrap();
	}

	#[test]
	fn the_parser_waits_for_data_and_drains_after_close() {
		let queue = Arc::new(OutputQueue::new(1024));
		let parser = {
			let queue = queue.clone();
			thread::spawn(move || {
				let mut out = Vec::new();
				let mut seen = Vec::new();
				while queue.wait_take(&mut out) {
					seen.extend_from_slice(&out);
				}
				seen
			})
		};
		thread::sleep(Duration::from_millis(20));
		queue.push(b"ab");
		queue.push(b"cd");
		queue.close();
		assert_eq!(parser.join().unwrap(), b"abcd");
	}
}

#[cfg(test)]
#[path = "queue_batch_tests.rs"]
mod batch_tests;

#[cfg(test)]
#[path = "queue_stream_tests.rs"]
mod stream_tests;

#[cfg(test)]
#[path = "queue_wake_tests.rs"]
mod wake_tests;
