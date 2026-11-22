//! Non-blocking input to the shell.
//!
//! Writes go through a channel to a dedicated thread. A blocking write on
//! the UI thread could deadlock: a large paste into a program that is busy
//! writing output fills the PTY in both directions while the UI thread,
//! the only consumer of that output, waits on the write.
use std::{
	io::Write,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
		mpsc::{self, Sender},
	},
	thread,
};

/// Most bytes of replies to queries that may wait to be written. A program
/// that sends queries but never reads its input would otherwise grow the
/// queue until memory runs out; past this its replies are dropped, as a
/// terminal whose reader is not listening loses them anyway.
pub(crate) const MAX_PENDING_REPLIES: usize = 1 << 20;

/// Cloneable: the UI thread sends input and the parser thread sends replies.
#[derive(Clone)]
pub(crate) struct PtyWriter {
	sender: Sender<(Vec<u8>, bool)>,
	/// Bytes of replies sent to the writer thread and not yet written.
	pending: Arc<AtomicUsize>,
}

impl PtyWriter {
	pub(crate) fn spawn(mut sink: Box<dyn Write + Send>) -> Self {
		let (sender, receiver) = mpsc::channel::<(Vec<u8>, bool)>();
		let pending = Arc::new(AtomicUsize::new(0));
		let written = pending.clone();
		thread::Builder::new()
			.name("pty-writer".into())
			.spawn(move || {
				for (bytes, reply) in receiver {
					let result = sink.write_all(&bytes).and_then(|()| sink.flush());
					if reply {
						written.fetch_sub(bytes.len(), Ordering::AcqRel);
					}
					if result.is_err() {
						break;
					}
				}
			})
			.expect("spawn PTY writer thread");
		Self { sender, pending }
	}

	/// Queues `bytes` for the shell without waiting for it to read them:
	/// what the user typed or pasted, which is never dropped.
	pub(crate) fn write(&self, bytes: &[u8]) {
		if !bytes.is_empty() {
			let _ = self.sender.send((bytes.to_vec(), false));
		}
	}

	/// Queues a reply to a query, unless [`MAX_PENDING_REPLIES`] bytes of
	/// replies already wait: then it is dropped whole.
	pub(crate) fn reply(&self, bytes: &[u8]) {
		let fits = |pending: usize| {
			(pending + bytes.len() <= MAX_PENDING_REPLIES).then_some(pending + bytes.len())
		};
		if bytes.is_empty()
			|| self
				.pending
				.try_update(Ordering::AcqRel, Ordering::Acquire, fits)
				.is_err()
		{
			return;
		}
		if self.sender.send((bytes.to_vec(), true)).is_err() {
			self.pending.fetch_sub(bytes.len(), Ordering::AcqRel);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		io::Read,
		time::{Duration, Instant},
	};

	#[test]
	fn writes_return_immediately_even_when_the_shell_is_not_reading() {
		let (mut reader, sink) = std::io::pipe().unwrap();
		let writer = PtyWriter::spawn(Box::new(sink));
		let payload: Vec<u8> = (0..1 << 20).map(|i| (i % 251) as u8).collect();
		let start = Instant::now();
		writer.write(&payload);
		writer.write(b"tail");
		assert!(
			start.elapsed() < Duration::from_millis(100),
			"write blocked the caller"
		);
		let mut received = vec![0; payload.len() + 4];
		reader.read_exact(&mut received).unwrap();
		assert_eq!(&received[..payload.len()], payload.as_slice());
		assert_eq!(&received[payload.len()..], b"tail");
	}
}
