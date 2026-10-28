//! Coalescing flushes on size, deadline and close, and leaves sparse output immediate.
use super::*;
use crate::pty::burst::TRIGGER;
use std::{
	sync::{Arc, atomic::Ordering, mpsc},
	thread,
	time::{Duration, Instant},
};

type Delivery = (bool, Vec<u8>);

fn active_queue(limit: usize, budget: Duration) -> Arc<OutputQueue> {
	let mut queue = OutputQueue::new(limit);
	queue.batch_wait = budget;
	queue.push(b"a");
	// Hold a long-lived lease so notification tests don't race a real 2 ms clock.
	queue
		.lock()
		.burst
		.received(TRIGGER, Instant::now() + Duration::from_secs(10));
	Arc::new(queue)
}

fn reader(queue: &Arc<OutputQueue>) -> (mpsc::Receiver<Delivery>, thread::JoinHandle<()>) {
	let (sender, receiver) = mpsc::channel();
	let queue = queue.clone();
	let worker = thread::spawn(move || {
		let mut bytes = Vec::new();
		let ok = queue.wait_take(&mut bytes);
		let _ = sender.send((ok, bytes));
	});
	(receiver, worker)
}

fn wait_for_batch(queue: &OutputQueue) -> bool {
	let deadline = Instant::now() + Duration::from_secs(2);
	while Instant::now() < deadline {
		if queue.coalescing.load(Ordering::Acquire) {
			return true;
		}
		thread::yield_now();
	}
	false
}

fn finish(
	queue: &OutputQueue,
	receiver: mpsc::Receiver<Delivery>,
	worker: thread::JoinHandle<()>,
) -> Result<Delivery, mpsc::RecvTimeoutError> {
	let result = receiver.recv_timeout(Duration::from_secs(2));
	queue.close();
	worker.join().unwrap();
	result
}

#[test]
fn a_partial_dense_batch_wakes_when_the_target_is_reached() {
	let queue = active_queue(1 << 20, Duration::from_secs(10));
	let (receiver, worker) = reader(&queue);
	let observed = wait_for_batch(&queue);
	let rest = vec![b'b'; (4 << 10) - 1];
	if observed {
		queue.push(&rest);
	} else {
		queue.close();
	}
	let result = finish(&queue, receiver, worker);
	assert!(observed, "reader never entered the coalescing wait");
	let mut expected = vec![b'a'];
	expected.extend_from_slice(&rest);
	assert_eq!(result.unwrap(), (true, expected));
}

#[test]
fn the_batch_target_never_exceeds_a_small_queue_limit() {
	let queue = active_queue(4, Duration::from_secs(10));
	let (receiver, worker) = reader(&queue);
	let observed = wait_for_batch(&queue);
	if observed {
		queue.push(b"bcd");
	} else {
		queue.close();
	}
	let result = finish(&queue, receiver, worker);
	assert!(observed);
	assert_eq!(result.unwrap(), (true, b"abcd".to_vec()));
}

#[test]
fn closing_flushes_a_partial_batch_without_waiting_for_its_deadline() {
	let queue = active_queue(1 << 20, Duration::from_secs(10));
	let (receiver, worker) = reader(&queue);
	let observed = wait_for_batch(&queue);
	queue.close();
	let result = finish(&queue, receiver, worker);
	assert!(observed);
	assert_eq!(result.unwrap(), (true, b"a".to_vec()));
	let mut out = vec![b'x'];
	assert!(!queue.wait_take(&mut out));
	assert!(out.is_empty());
}

#[test]
fn a_partial_batch_is_delivered_when_no_more_output_arrives() {
	let queue = active_queue(1 << 20, Duration::from_millis(25));
	let (receiver, worker) = reader(&queue);
	assert_eq!(
		finish(&queue, receiver, worker).unwrap(),
		(true, b"a".to_vec())
	);
}

#[test]
fn sparse_output_does_not_use_the_batch_wait_budget() {
	let mut queue = OutputQueue::new(1 << 20);
	queue.batch_wait = Duration::from_secs(10);
	queue.push(b"prompt$ ");
	let queue = Arc::new(queue);
	let (receiver, worker) = reader(&queue);
	let result = finish(&queue, receiver, worker);
	assert_eq!(result.unwrap(), (true, b"prompt$ ".to_vec()));
}

#[test]
fn a_closed_queue_with_data_bypasses_batching_immediately() {
	let queue = active_queue(1 << 20, Duration::from_secs(10));
	queue.close();
	let (receiver, worker) = reader(&queue);
	assert_eq!(
		finish(&queue, receiver, worker).unwrap(),
		(true, b"a".to_vec())
	);
}
