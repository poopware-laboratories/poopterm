//! Replies to a program that does not read its input: at most
//! `MAX_PENDING_REPLIES` bytes of them wait to be written, the rest are
//! dropped, while what the user types and pastes always gets through.
use super::writer::{MAX_PENDING_REPLIES, PtyWriter};
use std::{
	io::Read,
	sync::mpsc,
	thread,
	time::{Duration, Instant},
};

/// Everything written through `writer` into a pipe that is read only once
/// `writes` has run, with nothing read until then.
fn delivered(writes: impl FnOnce(&PtyWriter)) -> Vec<u8> {
	let (mut reader, sink) = std::io::pipe().unwrap();
	let writer = PtyWriter::spawn(Box::new(sink));
	writes(&writer);
	drop(writer);
	let mut out = Vec::new();
	reader.read_to_end(&mut out).unwrap();
	out
}

/// A device attributes reply, as a program flooding queries gets.
const REPLY: &[u8] = b"\x1b[?62;22c";

#[test]
fn replies_waiting_past_the_bound_are_dropped() {
	// Far more replies than fit, while nothing reads.
	let count = 4 * MAX_PENDING_REPLIES / REPLY.len();
	let out = delivered(|writer| (0..count).for_each(|_| writer.reply(REPLY)));
	assert!(
		out.len() < 2 * MAX_PENDING_REPLIES,
		"{} bytes delivered",
		out.len()
	);
	assert!(
		out.len() >= MAX_PENDING_REPLIES / 2,
		"most of the bound still arrives"
	);
	assert!(
		out.chunks(REPLY.len()).all(|reply| reply == REPLY),
		"whole replies only"
	);
}

#[test]
fn what_the_user_types_is_never_dropped() {
	let typed: Vec<u8> = (0..3 * MAX_PENDING_REPLIES)
		.map(|i| b'a' + (i % 26) as u8)
		.collect();
	let out = delivered(|writer| {
		(0..2 * MAX_PENDING_REPLIES / REPLY.len()).for_each(|_| writer.reply(REPLY));
		writer.write(&typed);
		writer.write(b"\r");
	});
	assert!(
		out.ends_with(&[&typed[..], b"\r"].concat()),
		"typed input arrives whole, last"
	);
}

/// Reads `reader` on a thread, handing on what arrives.
fn reading(mut reader: std::io::PipeReader) -> mpsc::Receiver<Vec<u8>> {
	let (send, received) = mpsc::channel();
	thread::spawn(move || {
		let mut buf = vec![0; 1 << 16];
		while let Ok(n @ 1..) = reader.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	received
}

/// What arrives on `received` until `last` ends it, or nothing more comes
/// for half a second.
fn until(received: &mpsc::Receiver<Vec<u8>>, last: &[u8]) -> Vec<u8> {
	let mut seen = Vec::new();
	while let Ok(bytes) = received.recv_timeout(Duration::from_millis(500)) {
		seen.extend(bytes);
		if seen.ends_with(last) {
			break;
		}
	}
	seen
}

#[test]
fn replies_go_through_again_once_the_program_reads() {
	let (reader, sink) = std::io::pipe().unwrap();
	let writer = PtyWriter::spawn(Box::new(sink));
	let received = reading(reader);
	// Twice: flood past the bound while the reader is behind, let it catch
	// up, then a reply of the same size must still fit.
	for round in 0..2 {
		for _ in 0..4 * MAX_PENDING_REPLIES / REPLY.len() {
			writer.reply(REPLY);
		}
		let start = Instant::now();
		while received.recv_timeout(Duration::from_millis(200)).is_ok() {
			assert!(
				start.elapsed() < Duration::from_secs(10),
				"round {round}: still draining"
			);
		}
		writer.reply(b"\x1b[?1;2c!");
		let late = until(&received, b"\x1b[?1;2c!");
		assert!(
			late.ends_with(b"\x1b[?1;2c!"),
			"round {round}: the budget came back"
		);
	}
}

#[test]
fn a_program_that_reads_gets_every_reply() {
	let (mut reader, sink) = std::io::pipe().unwrap();
	let writer = PtyWriter::spawn(Box::new(sink));
	let reading = thread::spawn(move || {
		let mut out = Vec::new();
		reader.read_to_end(&mut out).unwrap();
		out
	});
	// Spread out, as replies to an interactive program are.
	let count = 2000;
	for _ in 0..count {
		writer.reply(REPLY);
	}
	drop(writer);
	let out = reading.join().unwrap();
	assert_eq!(
		out.len(),
		count * REPLY.len(),
		"fewer than the bound: none dropped"
	);
}
