//! Learning that our children ended, from one blocking call, on each
//! system's own mechanism.
use super::*;
use crate::pty::{
	device::{self, Pair, WindowSize},
	spawn::{self, Child, Command, Exit},
};
use std::{
	sync::{Arc, mpsc},
	thread,
	time::Duration,
};

/// Starts `sh -c script` on a new PTY, kept open with it.
fn start(script: &str) -> (Child, Pair) {
	let pair = device::open(WindowSize::default()).unwrap();
	let child = spawn::spawn(&Command::new("sh").arg("-c").arg(script), &pair.slave).unwrap();
	(child, pair)
}

fn pid(child: &Child) -> libc::pid_t {
	child.id() as libc::pid_t
}

/// What `exits.wait()` returns on another thread within `timeout`, or
/// `None` if it is still blocked.
fn wait_within(exits: &Arc<Exits>, timeout: Duration) -> Option<Option<Vec<libc::pid_t>>> {
	let (send, got) = mpsc::channel();
	let exits = exits.clone();
	thread::spawn(move || {
		let _ = send.send(exits.wait().unwrap());
	});
	got.recv_timeout(timeout).ok()
}

/// Every pid `exits` reports until it has reported all of `pids`.
fn collect(exits: &Exits, pids: &[libc::pid_t]) -> Vec<libc::pid_t> {
	let mut seen = Vec::new();
	while !pids.iter().all(|pid| seen.contains(pid)) {
		seen.extend(exits.wait().unwrap().expect("not stopped"));
	}
	seen
}

#[test]
fn an_ended_child_is_reported() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("exit 3");
	if exits.add(pid(&child)).unwrap() {
		assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	}
	// Reported means ended: waiting does not block.
	assert_eq!(child.wait().unwrap(), Exit::Code(3));
}

#[test]
fn a_running_child_is_not_reported_until_it_ends() {
	let exits = Arc::new(Exits::new().unwrap());
	let (mut child, _pair) = start("read line");
	assert!(exits.add(pid(&child)).unwrap(), "it is still running");
	let (send, got) = mpsc::channel();
	let waiter = exits.clone();
	thread::spawn(move || {
		let _ = send.send(waiter.wait().unwrap());
	});
	assert!(
		got.recv_timeout(Duration::from_millis(200)).is_err(),
		"reported while running"
	);
	child.hang_up().unwrap();
	let reported = got
		.recv_timeout(Duration::from_secs(5))
		.expect("never reported");
	assert_eq!(reported, Some(vec![pid(&child)]));
	assert_eq!(child.wait().unwrap(), Exit::Signal(libc::SIGHUP));
}

#[test]
fn a_child_that_ended_before_it_was_added_is_reported_or_said_to_have_ended() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("exit 0");
	// Ended by now, but not reaped, so its pid is still ours. Linux and
	// FreeBSD report it at once; macOS says it has ended.
	thread::sleep(Duration::from_millis(300));
	if exits.add(pid(&child)).unwrap() {
		assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	}
	assert_eq!(child.wait().unwrap(), Exit::Code(0));
}

#[test]
fn many_children_are_each_reported_once() {
	let exits = Exits::new().unwrap();
	let children: Vec<_> = (0..20)
		.map(|n| start(&format!("sleep 0.0{}; exit {n}", n % 10)))
		.collect();
	let mut pids = Vec::new();
	let mut ended_before = Vec::new();
	for (child, _) in &children {
		if exits.add(pid(child)).unwrap() {
			pids.push(pid(child));
		} else {
			ended_before.push(pid(child));
		}
	}
	let mut seen = collect(&exits, &pids);
	seen.sort_unstable();
	let mut expected = pids.clone();
	expected.sort_unstable();
	assert_eq!(seen, expected, "each once, and only those added");
	for (n, (mut child, _pair)) in children.into_iter().enumerate() {
		assert_eq!(child.wait().unwrap(), Exit::Code(n as i32));
	}
}

#[test]
fn a_reported_child_is_not_reported_again() {
	let exits = Arc::new(Exits::new().unwrap());
	let (mut first, _one) = start("exit 1");
	let (mut second, _two) = start("read line");
	if exits.add(pid(&first)).unwrap() {
		assert_eq!(exits.wait().unwrap(), Some(vec![pid(&first)]));
	}
	first.wait().unwrap();
	assert!(exits.add(pid(&second)).unwrap());
	assert_eq!(
		wait_within(&exits, Duration::from_millis(200)),
		None,
		"the first came back"
	);
	second.hang_up().unwrap();
	second.wait().unwrap();
}

#[test]
fn stopping_wakes_a_blocked_wait() {
	let exits = Arc::new(Exits::new().unwrap());
	let (send, got) = mpsc::channel();
	let waiter = exits.clone();
	thread::spawn(move || {
		let _ = send.send(waiter.wait().unwrap());
	});
	thread::sleep(Duration::from_millis(50));
	exits.stop().unwrap();
	assert_eq!(got.recv_timeout(Duration::from_secs(5)).unwrap(), None);
}

#[test]
fn a_stopped_set_stays_stopped() {
	let exits = Exits::new().unwrap();
	exits.stop().unwrap();
	assert_eq!(exits.wait().unwrap(), None);
	assert_eq!(exits.wait().unwrap(), None, "the stop is not used up");
}

#[test]
fn stopping_wins_over_programs_that_ended() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("exit 0");
	let watched = exits.add(pid(&child)).unwrap();
	child.wait().unwrap();
	exits.stop().unwrap();
	assert_eq!(exits.wait().unwrap(), None, "watched: {watched}");
}

#[test]
fn a_process_that_does_not_exist_cannot_be_added() {
	let exits = Exits::new().unwrap();
	// Pids are positive and never this large on any system we build for.
	let error = exits.add(libc::pid_t::MAX).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ESRCH));
}

#[test]
fn separate_sets_report_only_their_own_children() {
	let (one, two) = (
		Arc::new(Exits::new().unwrap()),
		Arc::new(Exits::new().unwrap()),
	);
	let (mut first, _a) = start("exit 0");
	let (mut second, _b) = start("read line");
	let first_watched = one.add(pid(&first)).unwrap();
	assert!(two.add(pid(&second)).unwrap());
	first.wait().unwrap();
	assert_eq!(wait_within(&two, Duration::from_millis(200)), None);
	if first_watched {
		assert_eq!(one.wait().unwrap(), Some(vec![pid(&first)]));
	}
	second.hang_up().unwrap();
	assert_eq!(collect(&two, &[pid(&second)]), [pid(&second)]);
	second.wait().unwrap();
}
