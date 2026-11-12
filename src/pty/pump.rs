//! The parser thread's loop: applies queued PTY output to a shared terminal.
//!
//! Parsing happens off the UI thread so frames can render while output
//! streams in. Output is applied in bounded chunks so the UI never waits
//! long for the terminal lock.
use std::sync::atomic::{AtomicBool, Ordering};

use super::OutputQueue;
use crate::{parser::Parser, term::SharedTerm};

/// Bytes applied per lock acquisition.
const CHUNK: usize = 64 * 1024;

/// State the parser thread shares with the UI thread.
#[derive(Default)]
pub(crate) struct Flags {
	/// A wake-up was sent and the UI has not read the terminal since.
	pub notified: AtomicBool,
	/// The last batch was a backlog: output arrives faster than it parses.
	pub flooding: AtomicBool,
}

/// Parses everything pushed to `queue` into `term` until the queue is
/// closed and drained. Terminal replies go to `reply` right away; `wake`
/// runs once per batch, when `flags.notified` is not already set. The UI
/// clears it before it reads the terminal, so no update is missed.
pub(crate) fn pump(
	queue: &OutputQueue,
	term: &SharedTerm,
	flags: &Flags,
	mut reply: impl FnMut(&[u8]),
	mut wake: impl FnMut(),
) {
	let mut parser = Parser::new();
	let mut buffer = Vec::new();
	while queue.wait_take(&mut buffer) {
		let backlog = buffer.len() >= queue.limit() / 2;
		flags.flooding.store(backlog, Ordering::Release);
		for chunk in buffer.chunks(CHUNK) {
			term.yield_to_waiters();
			let responses = {
				let mut term = term.lock();
				parser.advance(&mut term, chunk);
				std::mem::take(&mut term.responses)
			};
			for response in responses {
				reply(&response);
			}
		}
		if !flags.notified.swap(true, Ordering::AcqRel) {
			wake();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		term::Term,
		theme::{Theme, ThemeKind},
	};
	use std::{sync::Arc, thread};

	fn shared() -> Arc<SharedTerm> {
		Arc::new(SharedTerm::new(Term::new(
			20,
			4,
			Theme::from_kind(ThemeKind::Poop),
			"/".into(),
		)))
	}

	fn first_row(term: &SharedTerm) -> String {
		term.lock().grid[0].iter().map(|cell| cell.ch).collect()
	}

	#[test]
	fn output_is_parsed_and_replies_are_sent() {
		let queue = OutputQueue::new(1 << 20);
		let term = shared();
		let flags = Flags::default();
		queue.push(b"hello\x1b[c");
		queue.close();
		let mut replies = Vec::new();
		let mut wakes = 0;
		pump(
			&queue,
			&term,
			&flags,
			|r| replies.push(r.to_vec()),
			|| wakes += 1,
		);
		assert!(first_row(&term).starts_with("hello"));
		assert_eq!(replies, [b"\x1b[?62;4c".to_vec()]);
		assert_eq!(wakes, 1);
	}

	#[test]
	fn wakeups_are_coalesced_until_the_ui_clears_the_flag() {
		let queue = Arc::new(OutputQueue::new(1 << 20));
		let term = shared();
		let flags = Arc::new(Flags::default());
		flags.notified.store(true, Ordering::Release);
		let (sender, receiver) = std::sync::mpsc::channel();
		let parser = {
			let (queue, term, flags) = (queue.clone(), term.clone(), flags.clone());
			thread::spawn(move || pump(&queue, &term, &flags, |_| {}, || sender.send(()).unwrap()))
		};
		queue.push(b"a");
		while !first_row(&term).starts_with('a') {
			thread::yield_now();
		}
		assert!(
			receiver.try_recv().is_err(),
			"UI has not consumed the last wake-up"
		);
		flags.notified.store(false, Ordering::Release);
		queue.push(b"b");
		receiver.recv().unwrap();
		queue.close();
		parser.join().unwrap();
		assert!(first_row(&term).starts_with("ab"));
	}

	#[test]
	fn a_backlog_of_output_is_reported_as_a_flood() {
		let term = shared();
		let flags = Flags::default();
		let queue = OutputQueue::new(1024);
		queue.push(&[b'x'; 600]);
		queue.close();
		pump(&queue, &term, &flags, |_| {}, || {});
		assert!(flags.flooding.load(Ordering::Acquire));
		let queue = OutputQueue::new(1024);
		queue.push(b"prompt$ ");
		queue.close();
		pump(&queue, &term, &flags, |_| {}, || {});
		assert!(!flags.flooding.load(Ordering::Acquire));
	}

	#[test]
	fn large_output_is_applied_in_chunks_without_losing_bytes() {
		let queue = OutputQueue::new(1 << 22);
		let term = shared();
		let flags = Flags::default();
		let mut payload = vec![b'x'; 3 * CHUNK + 7];
		payload.extend_from_slice("\r\né".as_bytes());
		// Split a multibyte character across the chunk boundary.
		let split = CHUNK - 1;
		payload.splice(split..split, "é".bytes());
		queue.push(&payload);
		queue.close();
		pump(&queue, &term, &flags, |_| {}, || {});
		let guard = term.lock();
		let last = guard.cy;
		assert_eq!(guard.grid[last][0].ch, 'é');
	}
}
