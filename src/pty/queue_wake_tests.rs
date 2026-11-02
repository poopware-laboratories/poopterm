//! When the queue wakes its waiters: the parser only when a push makes an
//! empty queue hold bytes (or, batching, reaches the batch target), the
//! reader only when a take drains a full queue. Every other notification
//! is a futex syscall that wakes nobody.
use super::{OutputQueue, wakes_parser};
use std::{
	sync::{Arc, mpsc},
	thread,
	time::Duration,
};

#[test]
fn only_a_push_into_an_empty_queue_wakes_the_parser() {
	let never = usize::MAX;
	assert!(wakes_parser(0, 1, never));
	assert!(wakes_parser(0, 64 << 10, never));
	// Already holding bytes: the parser is awake or about to take them.
	assert!(!wakes_parser(1, 2, never));
	assert!(!wakes_parser(4095, 4096, never));
	// An empty push leaves the queue empty; the parser sleeps on.
	assert!(!wakes_parser(0, 0, never));
	assert!(!wakes_parser(7, 7, never));
}

#[test]
fn a_batching_parser_is_also_woken_when_the_batch_target_is_reached() {
	assert!(wakes_parser(100, 4096, 4096), "reaches it");
	assert!(wakes_parser(4095, 9000, 4096), "passes it");
	assert!(!wakes_parser(100, 4095, 4096), "one short");
	assert!(!wakes_parser(4096, 5000, 4096), "already past it");
	assert!(wakes_parser(0, 10, 4096), "empty to nonempty still wakes");
}

/// The (data, space) notifications sent so far.
fn wakes(queue: &OutputQueue) -> (usize, usize) {
	queue.wakes()
}

#[test]
fn pushes_into_a_queue_with_bytes_send_no_notification() {
	let queue = OutputQueue::new(1024);
	for byte in 0..10u8 {
		queue.push(&[byte]);
	}
	assert_eq!(wakes(&queue), (1, 0), "only the first push");
	let mut out = Vec::new();
	assert!(queue.wait_take(&mut out));
	assert_eq!(out, (0..10).collect::<Vec<u8>>());
	// Ten bytes of 1024: the reader cannot be waiting for space.
	assert_eq!(wakes(&queue), (1, 0));
	queue.push(b"again");
	assert_eq!(wakes(&queue), (2, 0), "empty again, so woken again");
}

#[test]
fn only_draining_a_full_queue_wakes_the_reader() {
	let queue = OutputQueue::new(16);
	let mut out = Vec::new();
	queue.push(&[1; 15]);
	assert!(queue.wait_take(&mut out));
	assert_eq!(wakes(&queue).1, 0, "15 of 16");
	queue.push(&[2; 16]);
	assert!(queue.wait_take(&mut out));
	assert_eq!(wakes(&queue).1, 1, "full");
	// One read may carry the queue past its limit.
	queue.push(&[3; 40]);
	assert!(queue.wait_take(&mut out));
	assert_eq!((out.len(), wakes(&queue).1), (40, 2), "past full");
}

#[test]
fn a_reader_blocked_behind_an_oversized_read_is_released_by_the_take() {
	let queue = Arc::new(OutputQueue::new(8));
	// Below the limit, so it goes in whole, leaving the queue past it.
	queue.push(&[1; 20]);
	let (done_tx, done_rx) = mpsc::channel();
	let reader = {
		let queue = queue.clone();
		thread::spawn(move || {
			queue.push(&[2; 3]);
			done_tx.send(()).unwrap();
		})
	};
	assert!(
		done_rx.recv_timeout(Duration::from_millis(50)).is_err(),
		"the reader waits while the queue is past its limit"
	);
	let mut out = Vec::new();
	assert!(queue.wait_take(&mut out));
	assert_eq!(out, [1; 20]);
	done_rx
		.recv_timeout(Duration::from_secs(5))
		.expect("the take woke the reader");
	reader.join().unwrap();
	assert!(queue.wait_take(&mut out));
	assert_eq!(out, [2; 3]);
}

#[test]
fn a_parser_asleep_on_an_empty_queue_wakes_for_every_burst() {
	let queue = Arc::new(OutputQueue::new(1 << 10));
	let (tx, rx) = mpsc::channel();
	let parser = {
		let queue = queue.clone();
		thread::spawn(move || {
			let mut out = Vec::new();
			while queue.wait_take(&mut out) {
				tx.send(out.clone()).unwrap();
			}
		})
	};
	// Each burst lands on an empty queue the parser sleeps on: the only
	// push that notifies must reach it.
	for burst in 0..50u8 {
		thread::sleep(Duration::from_millis(2));
		queue.push(&[burst; 3]);
		let got = rx.recv_timeout(Duration::from_secs(5)).expect("woken");
		assert_eq!(got, [burst; 3]);
	}
	queue.close();
	parser.join().unwrap();
	assert_eq!(wakes(&queue).1, 0, "never full");
}

#[test]
fn a_flood_through_a_small_queue_keeps_every_byte_in_order_with_fewer_wakes() {
	let queue = Arc::new(OutputQueue::new(64));
	let producer = {
		let queue = queue.clone();
		thread::spawn(move || {
			for i in 0..20_000u32 {
				queue.push(&i.to_le_bytes()[..1 + (i % 4) as usize]);
			}
			queue.close();
		})
	};
	let mut out = Vec::new();
	let mut seen = Vec::new();
	while queue.wait_take(&mut out) {
		seen.extend_from_slice(&out);
	}
	producer.join().unwrap();
	let want: Vec<u8> = (0..20_000u32)
		.flat_map(|i| i.to_le_bytes()[..1 + (i % 4) as usize].to_vec())
		.collect();
	assert!(seen == want, "bytes lost or reordered");
	let (data, _) = wakes(&queue);
	assert!(data < 20_000, "{data} data notifications for 20000 pushes");
}
