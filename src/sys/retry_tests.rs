//! Trying again while every PTY is taken.
use super::*;
use std::time::Instant;

/// An attempt that fails with `errno` for its first `failures` calls,
/// counting every call in `calls`.
fn failing(
	errno: i32,
	failures: usize,
	calls: &mut usize,
) -> impl FnMut() -> io::Result<usize> + '_ {
	move || {
		*calls += 1;
		if *calls <= failures {
			Err(io::Error::from_raw_os_error(errno))
		} else {
			Ok(*calls)
		}
	}
}

#[test]
fn a_first_success_is_returned_after_one_try() {
	let mut calls = 0;
	let result = when_free(
		5,
		Duration::from_secs(60),
		failing(libc::ENXIO, 0, &mut calls),
	);
	assert_eq!(result.unwrap(), 1);
	assert_eq!(calls, 1);
}

#[test]
fn none_free_is_tried_again_until_one_is() {
	let mut calls = 0;
	let result = when_free(5, Duration::ZERO, failing(libc::ENXIO, 3, &mut calls));
	assert_eq!(result.unwrap(), 4);
}

#[test]
fn the_last_try_may_still_succeed() {
	let mut calls = 0;
	let result = when_free(5, Duration::ZERO, failing(libc::ENXIO, 4, &mut calls));
	assert_eq!(result.unwrap(), 5);
}

#[test]
fn after_all_its_tries_it_gives_the_error() {
	let mut calls = 0;
	let result = when_free(
		5,
		Duration::ZERO,
		failing(libc::ENXIO, usize::MAX, &mut calls),
	);
	assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::ENXIO));
	assert_eq!(calls, 5);
}

#[test]
fn other_errors_are_not_tried_again() {
	for errno in [libc::EMFILE, libc::EACCES, libc::EAGAIN, libc::ENOENT] {
		let mut calls = 0;
		let result = when_free(
			5,
			Duration::from_secs(60),
			failing(errno, usize::MAX, &mut calls),
		);
		assert_eq!(result.unwrap_err().raw_os_error(), Some(errno));
		assert_eq!(calls, 1, "errno {errno}");
	}
}

#[test]
fn a_single_try_never_waits() {
	let mut calls = 0;
	let began = Instant::now();
	let result = when_free(
		1,
		Duration::from_secs(60),
		failing(libc::ENXIO, usize::MAX, &mut calls),
	);
	assert!(result.is_err());
	assert_eq!(calls, 1);
	assert!(began.elapsed() < Duration::from_secs(1));
}

#[test]
fn tries_are_a_pause_apart_and_none_after_the_last() {
	let mut calls = 0;
	let began = Instant::now();
	let _ = when_free(
		4,
		Duration::from_millis(20),
		failing(libc::ENXIO, usize::MAX, &mut calls),
	);
	let waited = began.elapsed();
	// Three pauses between four tries; none after the last.
	assert!(waited >= Duration::from_millis(60), "{waited:?}");
	assert!(
		waited < Duration::from_millis(80) + Duration::from_millis(500),
		"{waited:?}"
	);
}
