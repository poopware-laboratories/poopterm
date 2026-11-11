//! Tests of process checks, hang-ups and waits, on children started with
//! `std::process` and waited for here instead.
use super::*;
use std::{process::Command, thread, time::Duration};

/// Starts `sh -c script` and returns its pid, for this module to wait for.
#[expect(clippy::zombie_processes, reason = "the tests reap it by pid")]
fn start(script: &str) -> i32 {
	let child = Command::new("sh").arg("-c").arg(script).spawn().unwrap();
	child.id() as i32
}

#[test]
fn we_exist_and_group_ids_never_do() {
	assert!(exists(std::process::id() as i32));
	// To `kill`, 0 is our own group and -1 every process we may signal.
	for group in [0, -1, -2, i32::MIN] {
		assert!(!exists(group), "{group}");
	}
}

#[test]
fn hanging_up_a_group_id_is_refused() {
	for group in [0, -1, i32::MIN] {
		let refused = hang_up(group).err().map(|error| error.kind());
		assert_eq!(refused, Some(io::ErrorKind::InvalidInput), "{group}");
	}
}

#[test]
fn exit_codes_come_back_whole() {
	for code in [0, 1, 3, 255] {
		let pid = start(&format!("exit {code}"));
		assert_eq!(wait(pid).unwrap(), Exit::Code(code), "exit {code}");
	}
}

#[test]
fn a_signal_that_ends_a_program_is_reported_as_one() {
	for (script, signal) in [
		("kill -KILL $$", libc::SIGKILL),
		("kill -TERM $$", libc::SIGTERM),
	] {
		assert_eq!(
			wait(start(script)).unwrap(),
			Exit::Signal(signal),
			"{script}"
		);
	}
}

#[test]
fn a_running_program_is_not_waited_for_and_a_hang_up_ends_it() {
	let pid = start("sleep 30");
	assert_eq!(try_wait(pid).unwrap(), None);
	assert!(exists(pid));
	hang_up(pid).unwrap();
	assert_eq!(wait(pid).unwrap(), Exit::Signal(libc::SIGHUP));
}

#[test]
fn an_ended_program_exists_until_it_is_waited_for() {
	let pid = start("exit 7");
	// Ended but not reaped: a zombie, which still holds its pid.
	thread::sleep(Duration::from_millis(200));
	assert!(exists(pid));
	assert_eq!(try_wait(pid).unwrap(), Some(Exit::Code(7)));
	assert!(!exists(pid));
}

#[test]
fn a_program_is_waited_for_only_once() {
	let pid = start("exit 0");
	assert_eq!(wait(pid).unwrap(), Exit::Code(0));
	for again in [wait(pid).err(), try_wait(pid).err()] {
		assert_eq!(
			again.and_then(|error| error.raw_os_error()),
			Some(libc::ECHILD)
		);
	}
	let gone = hang_up(pid).err().and_then(|error| error.raw_os_error());
	assert_eq!(gone, Some(libc::ESRCH));
}

#[test]
fn processes_that_are_not_our_children_cannot_be_waited_for() {
	// The init process is never our child.
	assert_eq!(
		wait(1).err().and_then(|error| error.raw_os_error()),
		Some(libc::ECHILD)
	);
	assert_eq!(
		try_wait(1).err().and_then(|error| error.raw_os_error()),
		Some(libc::ECHILD)
	);
}
