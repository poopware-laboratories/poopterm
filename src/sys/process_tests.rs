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

/// Sends `signal` (a name `kill` takes) to `pid` from outside, as another
/// program would.
fn send(signal: &str, pid: i32) {
	let status = Command::new("kill")
		.arg(format!("-{signal}"))
		.arg(pid.to_string())
		.status();
	assert!(status.unwrap().success());
}

#[test]
fn exit_codes_are_taken_modulo_256_as_the_kernel_keeps_them() {
	for (code, kept) in [(256, 0), (257, 1), (511, 255)] {
		assert_eq!(
			wait(start(&format!("exit {code}"))).unwrap(),
			Exit::Code(kept),
			"{code}"
		);
	}
}

#[test]
fn each_wait_reaps_only_the_child_it_names() {
	// Started together and waited for from threads in reverse order: a wait
	// for any child would hand some thread another's code.
	let pids: Vec<i32> = (0..6)
		.map(|code| start(&format!("sleep 0.1; exit {code}")))
		.collect();
	let waits: Vec<_> = pids
		.iter()
		.rev()
		.map(|&pid| thread::spawn(move || wait(pid)))
		.collect();
	for (code, waiter) in (0..6).rev().zip(waits) {
		assert_eq!(waiter.join().unwrap().unwrap(), Exit::Code(code));
	}
}

#[test]
fn a_pid_that_was_never_handed_out_does_not_exist() {
	// Far above any system's largest pid (Linux allows at most 2^22).
	assert!(!exists(i32::MAX));
	let gone = hang_up(i32::MAX)
		.err()
		.and_then(|error| error.raw_os_error());
	assert_eq!(gone, Some(libc::ESRCH));
}

#[test]
fn a_process_we_may_not_signal_does_not_count_as_existing() {
	if crate::sys::user::uid() == 0 {
		return; // root may signal init
	}
	assert!(!exists(1));
	let refused = hang_up(1).err().and_then(|error| error.raw_os_error());
	assert_eq!(refused, Some(libc::EPERM));
}

#[test]
fn a_program_that_ignores_hang_ups_keeps_running() {
	let pid = start("trap '' HUP; while :; do sleep 0.05; done");
	thread::sleep(Duration::from_millis(200));
	hang_up(pid).unwrap();
	thread::sleep(Duration::from_millis(200));
	assert_eq!(try_wait(pid).unwrap(), None, "SIGHUP, nothing stronger");
	send("KILL", pid);
	assert_eq!(wait(pid).unwrap(), Exit::Signal(libc::SIGKILL));
}

#[test]
fn a_stopped_program_has_not_ended() {
	let pid = start("kill -STOP $$; exit 4");
	thread::sleep(Duration::from_millis(200));
	assert_eq!(try_wait(pid).unwrap(), None);
	send("CONT", pid);
	assert_eq!(wait(pid).unwrap(), Exit::Code(4));
}

#[test]
fn polling_reports_the_end_once() {
	let pid = start("sleep 0.1; exit 9");
	let exit = loop {
		if let Some(exit) = try_wait(pid).unwrap() {
			break exit;
		}
		thread::sleep(Duration::from_millis(10));
	};
	assert_eq!(exit, Exit::Code(9));
	assert_eq!(
		try_wait(pid).err().and_then(|error| error.raw_os_error()),
		Some(libc::ECHILD)
	);
}

#[test]
fn a_program_that_ended_can_still_be_hung_up_until_it_is_reaped() {
	// A tab closing just after its shell exited: not an error.
	let pid = start("exit 2");
	thread::sleep(Duration::from_millis(200));
	hang_up(pid).unwrap();
	assert_eq!(
		wait(pid).unwrap(),
		Exit::Code(2),
		"the signal came too late to count"
	);
}
