//! The reader thread's loop: moves PTY output into the session's queue.

use std::io::{ErrorKind, Read};

use super::OutputQueue;

/// Bytes asked for per `read(2)`.
pub(crate) const READ_SIZE: usize = 1 << 16;

/// Hands everything `reader` yields to `sink` until end of file or an
/// error. A PTY whose shell exited reports EIO rather than end of file, so
/// errors end the stream too; interrupted reads are retried.
pub(crate) fn drain(mut reader: impl Read, mut sink: impl FnMut(&[u8])) {
	let mut buf = vec![0u8; READ_SIZE];
	loop {
		match reader.read(&mut buf) {
			Err(error) if error.kind() == ErrorKind::Interrupted => continue,
			Ok(0) | Err(_) => break,
			Ok(n) => sink(&buf[..n]),
		}
	}
}

/// Drains `reader` into `queue`, then closes the queue so the parser
/// thread finishes.
pub(crate) fn forward(reader: impl Read, queue: &OutputQueue) {
	drain(reader, |bytes| queue.push(bytes));
	queue.close();
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::VecDeque, io};

	/// A reader that hands out scripted results in order, then end of file.
	struct Script(VecDeque<io::Result<Vec<u8>>>);

	impl Read for Script {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			match self.0.pop_front() {
				None => Ok(0),
				Some(Err(error)) => Err(error),
				Some(Ok(bytes)) => {
					assert!(bytes.len() <= buf.len(), "a read never exceeds the buffer");
					buf[..bytes.len()].copy_from_slice(&bytes);
					Ok(bytes.len())
				}
			}
		}
	}

	fn script(steps: impl IntoIterator<Item = io::Result<&'static [u8]>>) -> Script {
		Script(
			steps
				.into_iter()
				.map(|step| step.map(<[u8]>::to_vec))
				.collect(),
		)
	}

	/// Everything the parser thread would take from `queue`, and whether
	/// the queue then reports itself closed.
	fn drained(queue: &OutputQueue) -> Vec<u8> {
		let (mut all, mut batch) = (Vec::new(), Vec::new());
		while queue.wait_take(&mut batch) {
			all.extend_from_slice(&batch);
		}
		all
	}

	#[test]
	fn output_arrives_in_order_and_the_queue_closes_at_the_end() {
		let queue = OutputQueue::new(1 << 20);
		forward(script([Ok(&b"ab"[..]), Ok(b"c"), Ok(b"def")]), &queue);
		assert_eq!(drained(&queue), b"abcdef");
	}

	#[test]
	fn an_interrupted_read_is_retried() {
		let queue = OutputQueue::new(1 << 20);
		let interrupted = io::Error::from(ErrorKind::Interrupted);
		forward(script([Ok(&b"a"[..]), Err(interrupted), Ok(b"b")]), &queue);
		assert_eq!(drained(&queue), b"ab");
	}

	#[test]
	fn an_exited_shell_ends_the_stream() {
		// Linux reports EIO on the master once the last slave closes.
		let queue = OutputQueue::new(1 << 20);
		let gone = io::Error::from_raw_os_error(libc::EIO);
		forward(script([Ok(&b"bye"[..]), Err(gone), Ok(b"never")]), &queue);
		assert_eq!(drained(&queue), b"bye");
	}

	#[test]
	fn nothing_to_read_still_closes_the_queue() {
		let queue = OutputQueue::new(1 << 20);
		forward(script([]), &queue);
		assert_eq!(drained(&queue), b"");
	}

	#[test]
	fn reads_ask_for_a_whole_buffer() {
		struct Sizes(Vec<usize>);
		impl Read for Sizes {
			fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
				self.0.push(buf.len());
				Ok(if self.0.len() < 3 { 1 } else { 0 })
			}
		}
		let queue = OutputQueue::new(1 << 20);
		let mut sizes = Sizes(Vec::new());
		forward(&mut sizes, &queue);
		assert_eq!(sizes.0, [READ_SIZE; 3]);
	}

	#[test]
	fn a_full_queue_waits_for_the_parser_instead_of_dropping_output() {
		// A queue holding 4 bytes at most: the reader must block on it, and
		// every byte still arrives once the parser drains.
		let queue = std::sync::Arc::new(OutputQueue::new(4));
		let data: Vec<u8> = (0..=255).cycle().take(10_000).collect();
		let reader = std::thread::spawn({
			let (queue, data) = (queue.clone(), data.clone());
			move || forward(io::Cursor::new(data), &queue)
		});
		assert_eq!(drained(&queue), data);
		reader.join().unwrap();
	}
}
