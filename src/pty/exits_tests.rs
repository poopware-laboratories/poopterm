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
fn only_a_positive_pid_names_a_program() {
	// `kill` takes 0 and below as process groups, ours among them; none of
	// them is a program that could end.
	let exits = Exits::new().unwrap();
	for pid in [0, -1, -2, libc::pid_t::MIN] {
		assert!(exits.add(pid).is_err(), "{pid} was taken");
	}
}

#[test]
fn an_end_is_reported_at_once_not_at_the_next_poll() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("read line");
	assert!(exits.add(pid(&child)).unwrap());
	thread::sleep(Duration::from_millis(50));
	let asked = std::time::Instant::now();
	child.hang_up().unwrap();
	assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	let took = asked.elapsed();
	assert!(took < Duration::from_millis(500), "reported after {took:?}");
	child.wait().unwrap();
}

#[test]
fn a_set_keeps_watching_as_programs_come_and_go() {
	let exits = Exits::new().unwrap();
	for n in 0..30 {
		let (mut child, _pair) = start(&format!("exit {}", n % 3));
		if exits.add(pid(&child)).unwrap() {
			assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]), "round {n}");
		}
		assert_eq!(child.wait().unwrap(), Exit::Code(n % 3));
	}
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

#[test]
fn more_ends_than_one_wait_takes_come_with_the_next() {
	let exits = Exits::new().unwrap();
	let children: Vec<_> = (0..BATCH * 2 + 3).map(|_| start("exit 0")).collect();
	// All ended before the first wait, so they are ready at once.
	thread::sleep(Duration::from_millis(300));
	let pids: Vec<_> = children
		.iter()
		.map(|(child, _)| pid(child))
		.filter(|&pid| exits.add(pid).unwrap())
		.collect();
	let first = if pids.is_empty() {
		Vec::new()
	} else {
		exits.wait().unwrap().unwrap()
	};
	assert!(first.len() <= BATCH, "{} in one wait", first.len());
	let rest: Vec<_> = pids
		.iter()
		.copied()
		.filter(|pid| !first.contains(pid))
		.collect();
	let mut seen = first;
	seen.extend(collect(&exits, &rest));
	seen.sort_unstable();
	let mut expected = pids;
	expected.sort_unstable();
	assert_eq!(seen, expected);
	for (mut child, _pair) in children {
		child.wait().unwrap();
	}
}

#[test]
fn a_child_added_twice_is_reported_once() {
	let exits = Arc::new(Exits::new().unwrap());
	let (mut child, _pair) = start("read line");
	assert!(exits.add(pid(&child)).unwrap());
	assert!(exits.add(pid(&child)).unwrap());
	child.hang_up().unwrap();
	assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	assert_eq!(
		wait_within(&exits, Duration::from_millis(200)),
		None,
		"reported twice"
	);
	child.wait().unwrap();
}

extern "C" fn ignore(_: libc::c_int) {}

#[test]
fn a_signal_that_interrupts_the_wait_does_not_end_it() {
	// A handler without SA_RESTART makes the blocking call fail with EINTR.
	// SAFETY: installs a handler that does nothing for SIGURG, which no
	// other test uses and which is ignored by default.
	unsafe {
		let mut action: libc::sigaction = std::mem::zeroed();
		action.sa_sigaction = ignore as extern "C" fn(libc::c_int) as libc::sighandler_t;
		assert_eq!(
			libc::sigaction(libc::SIGURG, &action, std::ptr::null_mut()),
			0
		);
	}
	let exits = Arc::new(Exits::new().unwrap());
	let (mut child, _pair) = start("read line");
	assert!(exits.add(pid(&child)).unwrap());
	let (send, got) = mpsc::channel();
	let waiter = exits.clone();
	let (thread_send, thread_got) = mpsc::channel();
	thread::spawn(move || {
		// As a number: musl's `pthread_t` is a pointer, which is not `Send`.
		// SAFETY: names the calling thread; takes nothing.
		let _ = thread_send.send(unsafe { libc::pthread_self() } as usize);
		let _ = send.send(waiter.wait());
	});
	let waiting = thread_got.recv().unwrap();
	for _ in 0..5 {
		thread::sleep(Duration::from_millis(20));
		// SAFETY: signals a thread that is alive: it is blocked in `wait`.
		unsafe { libc::pthread_kill(waiting as libc::pthread_t, libc::SIGURG) };
	}
	assert!(
		got.recv_timeout(Duration::from_millis(100)).is_err(),
		"the signal ended it"
	);
	child.hang_up().unwrap();
	let reported = got.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert_eq!(reported, Some(vec![pid(&child)]));
	child.wait().unwrap();
}

fn closes_on_exec(fd: &OwnedFd) -> bool {
	// SAFETY: reads the flags of a descriptor we own.
	let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
	flags >= 0 && flags & libc::FD_CLOEXEC != 0
}

#[test]
fn none_of_its_descriptors_reach_programs_we_start() {
	let exits = Exits::new().unwrap();
	assert!(closes_on_exec(&exits.set));
	assert!(closes_on_exec(&exits.stop.0) && closes_on_exec(&exits.stop.1));
	#[cfg(target_os = "linux")]
	{
		let (mut child, _pair) = start("read line");
		exits.add(pid(&child)).unwrap();
		let pidfds = exits.pidfds.lock().unwrap();
		assert!(pidfds.values().all(closes_on_exec));
		drop(pidfds);
		child.hang_up().unwrap();
		child.wait().unwrap();
	}
}

#[test]
fn two_threads_waiting_at_once_share_the_ends_without_repeats() {
	let exits = Arc::new(Exits::new().unwrap());
	let children: Vec<_> = (0..10).map(|_| start("read line")).collect();
	let pids: Vec<_> = children.iter().map(|(child, _)| pid(child)).collect();
	for &pid in &pids {
		assert!(exits.add(pid).unwrap());
	}
	let (send, got) = mpsc::channel();
	for _ in 0..2 {
		let (exits, send) = (exits.clone(), send.clone());
		thread::spawn(move || {
			while let Ok(Some(ended)) = exits.wait() {
				if send.send(ended).is_err() {
					return;
				}
			}
		});
	}
	for (child, _) in &children {
		child.hang_up().unwrap();
	}
	let mut seen = Vec::new();
	while seen.len() < pids.len() {
		let ended = got
			.recv_timeout(Duration::from_secs(5))
			.expect("an end is missing");
		assert!(!ended.is_empty(), "an empty report");
		seen.extend(ended);
	}
	assert!(
		got.recv_timeout(Duration::from_millis(100)).is_err(),
		"an end came twice"
	);
	seen.sort_unstable();
	let mut expected = pids;
	expected.sort_unstable();
	assert_eq!(seen, expected);
	exits.stop().unwrap();
	for (mut child, _pair) in children {
		child.wait().unwrap();
	}
}

#[test]
fn a_stopped_child_has_not_ended() {
	let exits = Arc::new(Exits::new().unwrap());
	let (mut child, _pair) = start("read line");
	assert!(exits.add(pid(&child)).unwrap());
	// SAFETY: signals only the child we started.
	unsafe { libc::kill(pid(&child), libc::SIGSTOP) };
	assert_eq!(
		wait_within(&exits, Duration::from_millis(200)),
		None,
		"a stop is no end"
	);
	// SAFETY: as above.
	unsafe { libc::kill(pid(&child), libc::SIGKILL) };
	assert_eq!(collect(&exits, &[pid(&child)]), [pid(&child)]);
	assert_eq!(child.wait().unwrap(), Exit::Signal(libc::SIGKILL));
}

#[test]
fn a_child_that_becomes_another_program_is_still_watched() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("exec sh -c 'sleep 0.1; exit 5'");
	assert!(exits.add(pid(&child)).unwrap());
	assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	assert_eq!(child.wait().unwrap(), Exit::Code(5));
}

#[test]
fn a_grandchild_ending_is_not_its_parents_end() {
	let exits = Arc::new(Exits::new().unwrap());
	let (mut child, _pair) = start("sleep 0.05 & read line");
	assert!(exits.add(pid(&child)).unwrap());
	assert_eq!(
		wait_within(&exits, Duration::from_millis(300)),
		None,
		"the sleep's end counted"
	);
	child.hang_up().unwrap();
	assert_eq!(collect(&exits, &[pid(&child)]), [pid(&child)]);
	child.wait().unwrap();
}

#[test]
fn children_added_from_many_threads_while_another_waits_all_arrive() {
	let exits = Arc::new(Exits::new().unwrap());
	let (send, got) = mpsc::channel();
	let waiter = exits.clone();
	thread::spawn(move || {
		while let Ok(Some(ended)) = waiter.wait() {
			if send.send(ended).is_err() {
				return;
			}
		}
	});
	let adders: Vec<_> = (0..8)
		.map(|_| {
			let exits = exits.clone();
			thread::spawn(move || {
				let (child, pair) = start("sleep 0.05");
				let watched = exits.add(pid(&child)).unwrap();
				(pid(&child), watched, child, pair)
			})
		})
		.collect();
	let started: Vec<_> = adders
		.into_iter()
		.map(|adder| adder.join().unwrap())
		.collect();
	let mut expected: Vec<_> = started.iter().filter(|s| s.1).map(|s| s.0).collect();
	let mut seen = Vec::new();
	while seen.len() < expected.len() {
		seen.extend(
			got
				.recv_timeout(Duration::from_secs(5))
				.expect("an end is missing"),
		);
	}
	seen.sort_unstable();
	expected.sort_unstable();
	assert_eq!(seen, expected);
	exits.stop().unwrap();
	for (_, _, mut child, _pair) in started {
		child.wait().unwrap();
	}
}

#[test]
fn one_stop_wakes_every_waiter() {
	let exits = Arc::new(Exits::new().unwrap());
	let (send, got) = mpsc::channel();
	for _ in 0..3 {
		let (exits, send) = (exits.clone(), send.clone());
		thread::spawn(move || {
			let _ = send.send(exits.wait().unwrap());
		});
	}
	thread::sleep(Duration::from_millis(50));
	exits.stop().unwrap();
	for _ in 0..3 {
		assert_eq!(got.recv_timeout(Duration::from_secs(5)).unwrap(), None);
	}
}

#[test]
fn stopping_twice_or_adding_after_a_stop_keeps_it_stopped() {
	let exits = Exits::new().unwrap();
	exits.stop().unwrap();
	exits.stop().unwrap();
	let (mut child, _pair) = start("exit 0");
	let _ = exits.add(pid(&child)).unwrap();
	child.wait().unwrap();
	assert_eq!(exits.wait().unwrap(), None);
}

#[cfg(target_os = "linux")]
#[test]
fn no_pidfd_is_kept_once_its_program_is_reported() {
	let exits = Exits::new().unwrap();
	let (mut child, _pair) = start("exit 0");
	exits.add(pid(&child)).unwrap();
	assert_eq!(exits.pidfds.lock().unwrap().len(), 1);
	assert_eq!(exits.wait().unwrap(), Some(vec![pid(&child)]));
	assert!(
		exits.pidfds.lock().unwrap().is_empty(),
		"a descriptor per ended tab leaks"
	);
	child.wait().unwrap();
}
