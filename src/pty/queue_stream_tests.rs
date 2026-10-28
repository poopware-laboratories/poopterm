//! Queue transition regressions: blocked readers, drain/close, and byte ordering.
use super::OutputQueue;
use std::{
	sync::{Arc, mpsc},
	thread,
	time::Duration,
};

#[test]
fn each_empty_to_nonempty_transition_releases_a_waiting_parser() {
	let queue = Arc::new(OutputQueue::new(16));
	let (ready_tx, ready_rx) = mpsc::channel();
	let (data_tx, data_rx) = mpsc::channel();
	let reader = {
		let queue = queue.clone();
		thread::spawn(move || {
			let mut bytes = Vec::new();
			ready_tx.send(()).unwrap();
			while queue.wait_take(&mut bytes) {
				if data_tx.send(bytes.clone()).is_err() {
					break;
				}
			}
		})
	};
	ready_rx.recv().unwrap();
	thread::sleep(Duration::from_millis(20));
	let mut seen = Vec::new();
	let mut error = None;
	for value in 0..32u8 {
		queue.push(&[value]);
		match data_rx.recv_timeout(Duration::from_secs(2)) {
			Ok(bytes) => seen.extend(bytes),
			Err(failure) => {
				error = Some(failure);
				break;
			}
		}
	}
	queue.close();
	reader.join().unwrap();
	assert!(error.is_none(), "missed data wakeup: {error:?}");
	assert_eq!(seen, (0..32u8).collect::<Vec<_>>());
}

#[test]
fn draining_a_full_queue_releases_a_blocked_producer_without_another_push() {
	let queue = Arc::new(OutputQueue::new(4));
	queue.push(b"abcd");
	let (ready_tx, ready_rx) = mpsc::channel();
	let (done_tx, done_rx) = mpsc::channel();
	let writer = {
		let queue = queue.clone();
		thread::spawn(move || {
			ready_tx.send(()).unwrap();
			queue.push(b"ef");
			done_tx.send(()).unwrap();
		})
	};
	ready_rx.recv().unwrap();
	assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());
	let mut bytes = Vec::new();
	assert!(queue.wait_take(&mut bytes));
	assert_eq!(bytes, b"abcd");
	let released = done_rx.recv_timeout(Duration::from_secs(2));
	queue.close();
	writer.join().unwrap();
	assert!(released.is_ok(), "missed space wakeup");
	assert!(queue.wait_take(&mut bytes));
	assert_eq!(bytes, b"ef");
	assert!(!queue.wait_take(&mut bytes));
	assert!(bytes.is_empty());
}

#[test]
fn repeated_bursts_preserve_every_byte_under_backpressure() {
	let queue = Arc::new(OutputQueue::new(127));
	let expected: Vec<u8> = (0..65536).map(|i| (i * 37) as u8).collect();
	let writer = {
		let queue = queue.clone();
		let input = expected.clone();
		thread::spawn(move || {
			for chunk in input.chunks(19) {
				queue.push(chunk);
			}
			queue.close();
		})
	};
	let mut received = Vec::new();
	let mut bytes = Vec::new();
	while queue.wait_take(&mut bytes) {
		// A push can exceed the limit by at most the current chunk.
		assert!(bytes.len() < 127 + 19);
		received.extend_from_slice(&bytes);
	}
	writer.join().unwrap();
	assert_eq!(received, expected);
}

#[test]
fn pushes_before_a_take_are_coalesced_and_close_keeps_the_final_batch() {
	let queue = OutputQueue::new(32);
	queue.push(b"first");
	queue.push(b"-");
	queue.push(b"second");
	queue.close();
	let mut bytes = Vec::new();
	assert!(queue.wait_take(&mut bytes));
	assert_eq!(bytes, b"first-second");
	assert!(!queue.wait_take(&mut bytes));
	assert!(bytes.is_empty());
}

#[test]
fn empty_pushes_do_not_finish_a_waiting_parser() {
	let queue = Arc::new(OutputQueue::new(8));
	let (tx, rx) = mpsc::channel();
	let parser = {
		let queue = queue.clone();
		thread::spawn(move || {
			let mut bytes = Vec::new();
			tx.send((queue.wait_take(&mut bytes), bytes)).unwrap();
		})
	};
	queue.push(b"");
	let early = rx.recv_timeout(Duration::from_millis(20));
	queue.close();
	let finished = if early.is_ok() {
		None
	} else {
		rx.recv_timeout(Duration::from_secs(2)).ok()
	};
	parser.join().unwrap();
	assert!(early.is_err());
	assert_eq!(finished, Some((false, vec![])));
}

#[test]
fn active_batching_preserves_fragmented_bytes_under_backpressure() {
	let mut queue = OutputQueue::new(512);
	queue.batch_wait = Duration::from_micros(50);
	queue.lock().burst.received(
		crate::pty::burst::TRIGGER,
		std::time::Instant::now() + Duration::from_secs(10),
	);
	let queue = Arc::new(queue);
	let expected: Vec<u8> = (0..262144).map(|i| (i * 71) as u8).collect();
	let writer = {
		let queue = queue.clone();
		let input = expected.clone();
		thread::spawn(move || {
			for chunk in input.chunks(73) {
				queue.push(chunk);
			}
			queue.close();
		})
	};
	let mut seen = Vec::new();
	let mut bytes = Vec::new();
	while queue.wait_take(&mut bytes) {
		assert!(bytes.len() < 512 + 73);
		seen.extend_from_slice(&bytes);
	}
	writer.join().unwrap();
	assert_eq!(seen, expected);
	assert!(!queue.wait_take(&mut bytes));
	assert!(bytes.is_empty());
}
