//! Running a program a test has just written. While the test writes it,
//! another test thread may fork; the child keeps a copy of the write
//! descriptor until it executes its own program, and executing a file open
//! for writing anywhere fails with ETXTBSY. The window is short, so trying
//! again shortly afterwards gets past it.
use std::{io, thread, time::Duration};

/// Tries before giving up, 10 ms apart: a second in all.
const TRIES: usize = 100;

/// `attempt`'s result, trying again while it fails with ETXTBSY.
pub(crate) fn retry_busy<T>(mut attempt: impl FnMut() -> io::Result<T>) -> io::Result<T> {
	for _ in 1..TRIES {
		match attempt() {
			Err(error) if error.kind() == io::ErrorKind::ExecutableFileBusy => {
				thread::sleep(Duration::from_millis(10));
			}
			result => return result,
		}
	}
	attempt()
}

/// What `stderr` shows when a shell could not run a busy program.
pub(crate) fn busy_in(stderr: &[u8]) -> bool {
	String::from_utf8_lossy(stderr).contains("Text file busy")
}

#[test]
fn a_busy_program_is_tried_again_until_it_runs() {
	let mut tries = 0;
	let result = retry_busy(|| {
		tries += 1;
		if tries < 4 {
			Err(io::ErrorKind::ExecutableFileBusy.into())
		} else {
			Ok(tries)
		}
	});
	assert_eq!(result.unwrap(), 4);
}

#[test]
fn other_errors_and_successes_return_at_once() {
	let mut tries = 0;
	let error = retry_busy(|| -> io::Result<()> {
		tries += 1;
		Err(io::ErrorKind::NotFound.into())
	});
	assert_eq!(error.unwrap_err().kind(), io::ErrorKind::NotFound);
	assert_eq!(tries, 1);
	assert_eq!(retry_busy(|| Ok(7)).unwrap(), 7);
}

#[test]
fn a_program_busy_for_good_fails_after_every_try() {
	let mut tries = 0;
	let error = retry_busy(|| -> io::Result<()> {
		tries += 1;
		Err(io::Error::from_raw_os_error(libc::ETXTBSY))
	});
	assert_eq!(error.unwrap_err().kind(), io::ErrorKind::ExecutableFileBusy);
	assert_eq!(tries, TRIES);
}

#[test]
fn only_the_busy_message_counts_as_busy() {
	assert!(busy_in(b"bash: line 1: /tmp/x/ssh: Text file busy\n"));
	assert!(!busy_in(b"bash: ssh: command not found\n"));
	assert!(!busy_in(b""));
}

#[test]
fn a_script_written_while_others_fork_still_runs() {
	use std::os::unix::fs::PermissionsExt;
	// Children forked from other threads while scripts are written: the
	// race this module exists for, made likely.
	let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
	let forks: Vec<_> = (0..4)
		.map(|_| {
			let stop = stop.clone();
			thread::spawn(move || {
				while !stop.load(std::sync::atomic::Ordering::Relaxed) {
					let _ = std::process::Command::new("true").status();
				}
			})
		})
		.collect();
	let dir = crate::scratch_tests::Scratch::new("busy", "race");
	for n in 0..50 {
		let script = dir.join(format!("s{n}"));
		std::fs::write(&script, "#!/bin/sh\necho ran\n").unwrap();
		std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
		let output = retry_busy(|| std::process::Command::new(&script).output()).unwrap();
		assert_eq!(output.stdout, b"ran\n", "script {n}");
	}
	stop.store(true, std::sync::atomic::Ordering::Relaxed);
	for fork in forks {
		fork.join().unwrap();
	}
}
