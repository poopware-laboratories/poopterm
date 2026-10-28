//! Dense-burst detection is bounded in time and never accumulates sparse traffic.
use super::{Burst, TRIGGER, WINDOW};
use std::time::{Duration, Instant};

#[test]
fn a_fresh_detector_and_empty_input_are_inactive() {
	let start = Instant::now();
	let mut burst = Burst::default();
	assert!(!burst.active(start));
	burst.received(0, start);
	assert!(!burst.active(start));
	burst.received(0, start + WINDOW);
	assert!(!burst.active(start + WINDOW));
}

#[test]
fn threshold_is_inclusive_and_the_lease_expires_at_its_boundary() {
	let start = Instant::now();
	let mut burst = Burst::default();
	burst.received(TRIGGER - 1, start);
	assert!(!burst.active(start));
	let last = start + Duration::from_micros(1);
	burst.received(1, last);
	assert!(burst.active(last));
	assert!(burst.active(last + WINDOW - Duration::from_nanos(1)));
	assert!(!burst.active(last + WINDOW));
	assert!(!burst.active(last + WINDOW + Duration::from_nanos(1)));
}

#[test]
fn many_small_reads_in_one_window_form_a_dense_burst() {
	let start = Instant::now();
	let mut burst = Burst::default();
	for i in 0..TRIGGER / 1024 - 1 {
		burst.received(1024, start + Duration::from_micros(i as u64));
		assert!(!burst.active(start + Duration::from_micros(i as u64)));
	}
	let last = start + Duration::from_micros((TRIGGER / 1024) as u64);
	burst.received(1024, last);
	assert!(burst.active(last));
}

#[test]
fn bytes_from_expired_windows_cannot_be_accumulated() {
	let start = Instant::now();
	let mut burst = Burst::default();
	burst.received(TRIGGER - 1, start);
	burst.received(1, start + WINDOW);
	assert!(!burst.active(start + WINDOW));
	burst.received(TRIGGER - 2, start + WINDOW + Duration::from_micros(1));
	assert!(!burst.active(start + WINDOW + Duration::from_micros(1)));
	burst.received(1, start + WINDOW + Duration::from_micros(2));
	assert!(burst.active(start + WINDOW + Duration::from_micros(2)));
}

#[test]
fn sparse_interactive_output_never_activates_batching() {
	let start = Instant::now();
	let mut burst = Burst::default();
	for i in 0..10_000 {
		let now = start + Duration::from_millis(i);
		burst.received(64, now);
		assert!(!burst.active(now));
	}
}

#[test]
fn a_quiet_interval_stops_batching_without_an_extra_event() {
	let start = Instant::now();
	let mut burst = Burst::default();
	burst.received(TRIGGER, start);
	assert!(burst.active(start));
	assert!(!burst.active(start + WINDOW));
	let later = start + WINDOW * 10;
	burst.received(1, later);
	assert!(!burst.active(later));
}

#[test]
fn zero_length_pushes_do_not_extend_an_active_lease() {
	let start = Instant::now();
	let mut burst = Burst::default();
	burst.received(TRIGGER, start);
	burst.received(0, start + WINDOW - Duration::from_nanos(1));
	assert!(!burst.active(start + WINDOW));
	burst.received(0, start + WINDOW * 2);
	assert!(!burst.active(start + WINDOW * 2));
}

#[test]
fn sustained_dense_windows_keep_the_lease_active() {
	let start = Instant::now();
	let mut burst = Burst::default();
	for i in 0..100 {
		let now = start + Duration::from_millis(i);
		burst.received(TRIGGER, now);
		assert!(burst.active(now));
		assert!(burst.active(now + Duration::from_micros(999)));
	}
	assert!(!burst.active(start + Duration::from_millis(101)));
}

#[test]
fn counters_saturate_without_losing_the_dense_state() {
	let start = Instant::now();
	let mut burst = Burst::default();
	burst.received(usize::MAX, start);
	burst.received(1, start + Duration::from_micros(1));
	assert!(burst.active(start + Duration::from_micros(1)));
	assert!(!burst.active(start + WINDOW * 2));
}

#[test]
fn one_large_read_is_immediately_dense() {
	let start = Instant::now();
	for bytes in [TRIGGER, TRIGGER + 1, TRIGGER * 16] {
		let mut burst = Burst::default();
		burst.received(bytes, start);
		assert!(burst.active(start));
		assert!(!burst.active(start + WINDOW));
	}
}
