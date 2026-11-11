//! One reaper for many children: each one's end reaches its own code, once,
//! and nothing is left unreaped.
use super::*;
use crate::pty::{
	device::{self, Pair, WindowSize},
	spawn::{self, Command},
};
use std::{
	sync::mpsc::{self, Receiver},
	thread,
	time::Duration,
};

/// Starts `sh -c script` on a new PTY, kept open with it.
fn start(script: &str) -> (Child, Pair) {
	let pair = device::open(WindowSize::default()).unwrap();
	let child = spawn::spawn(&Command::new("sh").arg("-c").arg(script), &pair.slave).unwrap();
	(child, pair)
}

/// Watches `child` on `reaper`; its end arrives on the receiver.
fn watched(reaper: &Reaper, child: Child) -> Receiver<io::Result<Exit>> {
	let (send, ended) = mpsc::channel();
	reaper.watch(child, move |exit| {
		let _ = send.send(exit);
	});
	ended
}

fn within(ended: &Receiver<io::Result<Exit>>, seconds: u64) -> Exit {
	ended
		.recv_timeout(Duration::from_secs(seconds))
		.expect("its end never came")
		.unwrap()
}

/// Whether `pid` is a child of ours not yet reaped, without reaping it.
fn unreaped(pid: libc::pid_t) -> bool {
	// SAFETY: signal 0 only checks that the process exists; a zombie does.
	unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn a_childs_exit_code_reaches_its_code() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("exit 3");
	assert_eq!(within(&watched(&reaper, child), 5), Exit::Code(3));
}

#[test]
fn a_signal_that_ended_a_child_reaches_its_code() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("kill -TERM $$");
	assert_eq!(
		within(&watched(&reaper, child), 5),
		Exit::Signal(libc::SIGTERM)
	);
}

#[test]
fn nothing_arrives_while_a_child_runs() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("read line");
	let pid = child.id() as libc::pid_t;
	let ended = watched(&reaper, child);
	assert!(ended.recv_timeout(Duration::from_millis(200)).is_err());
	// SAFETY: signals only the child we started, which is still running.
	unsafe { libc::kill(pid, libc::SIGHUP) };
	assert_eq!(within(&ended, 5), Exit::Signal(libc::SIGHUP));
}

#[test]
fn a_child_that_ended_before_it_was_watched_still_arrives() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("exit 4");
	thread::sleep(Duration::from_millis(300));
	assert_eq!(within(&watched(&reaper, child), 5), Exit::Code(4));
}

#[test]
fn a_reaped_child_leaves_no_zombie() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("exit 0");
	let pid = child.id() as libc::pid_t;
	within(&watched(&reaper, child), 5);
	assert!(!unreaped(pid), "{pid} is still a zombie");
}

#[test]
fn one_reaper_serves_many_children_each_once() {
	let reaper = Reaper::new().unwrap();
	let (send, ended) = mpsc::channel();
	let mut pairs = Vec::new();
	for n in 0..20 {
		let (child, pair) = start(&format!("sleep 0.0{}; exit {n}", n % 10));
		pairs.push(pair);
		let send = send.clone();
		reaper.watch(child, move |exit| {
			let _ = send.send((n, exit.unwrap()));
		});
	}
	drop(send);
	let mut seen: Vec<_> = (0..20)
		.map(|_| {
			ended
				.recv_timeout(Duration::from_secs(10))
				.expect("missing an end")
		})
		.collect();
	seen.sort_unstable_by_key(|(n, _)| *n);
	let expected: Vec<_> = (0..20).map(|n| (n, Exit::Code(n))).collect();
	assert_eq!(seen, expected);
	assert!(
		ended.recv_timeout(Duration::from_millis(100)).is_err(),
		"an end came twice"
	);
}

#[test]
fn children_may_be_watched_from_many_threads() {
	let reaper = std::sync::Arc::new(Reaper::new().unwrap());
	let threads: Vec<_> = (0..8)
		.map(|n| {
			let reaper = reaper.clone();
			thread::spawn(move || {
				let (child, _pair) = start(&format!("exit {n}"));
				within(&watched(&reaper, child), 10)
			})
		})
		.collect();
	for (n, thread) in threads.into_iter().enumerate() {
		assert_eq!(thread.join().unwrap(), Exit::Code(n as i32));
	}
}

#[test]
fn the_code_runs_on_the_reapers_thread_not_the_watchers() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("sleep 0.1");
	let (send, ran_on) = mpsc::channel();
	reaper.watch(child, move |_| {
		let current = thread::current();
		let _ = send.send((current.id(), current.name().map(str::to_owned)));
	});
	let (id, name) = ran_on.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_ne!(id, thread::current().id());
	// Named, so a profile or a debugger shows what it is.
	assert_eq!(name.as_deref(), Some("pty-reaper"));
}

#[cfg(target_os = "linux")]
#[test]
fn no_descriptor_is_kept_for_children_that_ended() {
	let reaper = Reaper::new().unwrap();
	let mut pairs = Vec::new();
	let (send, ended) = mpsc::channel();
	for _ in 0..10 {
		let (child, pair) = start("exit 0");
		pairs.push(pair);
		let send = send.clone();
		reaper.watch(child, move |exit| {
			let _ = send.send(exit.unwrap());
		});
	}
	for _ in 0..10 {
		within_ok(&ended);
	}
	assert!(reaper.shared.exits.pidfds.lock().unwrap().is_empty());
	assert!(reaper.shared.lock().is_empty(), "every child left the map");
}

#[test]
fn dropping_the_reaper_frees_the_code_it_still_held() {
	let held = std::sync::Arc::new(());
	let reaper = Reaper::new().unwrap();
	let (child, pair) = start("read line");
	let pid = child.id() as libc::pid_t;
	let inside = held.clone();
	reaper.watch(child, move |_| drop(inside));
	assert_eq!(std::sync::Arc::strong_count(&held), 2);
	drop(reaper);
	assert_eq!(
		std::sync::Arc::strong_count(&held),
		1,
		"the code outlived its reaper"
	);
	// SAFETY: kills the child we started, which nothing reaps now.
	unsafe { libc::kill(pid, libc::SIGKILL) };
	// SAFETY: reaps it; no status is written.
	unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
	drop(pair);
}

#[test]
fn dropping_the_reaper_stops_its_thread() {
	let reaper = Reaper::new().unwrap();
	let (done, dropped) = mpsc::channel();
	thread::spawn(move || {
		drop(reaper);
		let _ = done.send(());
	});
	assert!(
		dropped.recv_timeout(Duration::from_secs(5)).is_ok(),
		"drop hung"
	);
}

#[test]
fn a_child_still_running_when_the_reaper_drops_is_not_reported() {
	let reaper = Reaper::new().unwrap();
	let (child, pair) = start("read line");
	let pid = child.id() as libc::pid_t;
	let ended = watched(&reaper, child);
	drop(reaper);
	assert!(ended.recv_timeout(Duration::from_millis(100)).is_err());
	// SAFETY: signals the child we started; nothing will reap it now.
	unsafe { libc::kill(pid, libc::SIGKILL) };
	let mut status = 0;
	// SAFETY: reaps our own child into an integer we own.
	assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
	drop(pair);
}

#[test]
fn a_child_someone_else_reaped_reports_the_error() {
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("read line");
	let pid = child.id() as libc::pid_t;
	let (send, ended) = mpsc::channel();
	// SAFETY: kills our own child behind the reaper's back.
	unsafe { libc::kill(pid, libc::SIGKILL) };
	// SAFETY: reaps it; no status is written.
	unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
	reaper.watch(child, move |exit| {
		let _ = send.send(exit);
	});
	let exit = ended
		.recv_timeout(Duration::from_secs(5))
		.expect("never reported");
	assert!(exit.is_err(), "{exit:?}");
}

#[test]
fn a_reaper_dropped_by_its_own_code_does_not_wait_for_itself() {
	use std::sync::{Arc, Mutex};
	let slot = Arc::new(Mutex::new(Some(Reaper::new().unwrap())));
	let (child, _pair) = start("sleep 0.1");
	let (send, done) = mpsc::channel();
	let inside = slot.clone();
	slot
		.lock()
		.unwrap()
		.as_ref()
		.unwrap()
		.watch(child, move |_| {
			// The last reaper goes away on its own thread.
			let last = inside.lock().unwrap().take();
			drop(last);
			let _ = send.send(());
		});
	assert!(
		done.recv_timeout(Duration::from_secs(5)).is_ok(),
		"the drop hung"
	);
	assert!(slot.lock().unwrap().is_none());
}

#[test]
fn code_may_watch_the_next_child_as_one_ends() {
	// As a tab that restarts its shell would.
	let reaper = std::sync::Arc::new(Reaper::new().unwrap());
	let (child, _first) = start("exit 1");
	let (send, ended) = mpsc::channel();
	let (pairs, kept) = mpsc::channel();
	let again = std::sync::Arc::downgrade(&reaper);
	reaper.watch(child, move |first| {
		let _ = send.send(first.unwrap());
		let (next, pair) = start("exit 2");
		let _ = pairs.send(pair);
		let send = send.clone();
		if let Some(reaper) = again.upgrade() {
			reaper.watch(next, move |second| {
				let _ = send.send(second.unwrap());
			});
		}
	});
	assert_eq!(within_ok(&ended), Exit::Code(1));
	assert_eq!(within_ok(&ended), Exit::Code(2));
	drop(kept);
}

fn within_ok(ended: &Receiver<Exit>) -> Exit {
	ended
		.recv_timeout(Duration::from_secs(5))
		.expect("its end never came")
}

#[test]
fn every_exit_code_and_signal_comes_through_unchanged() {
	let reaper = Reaper::new().unwrap();
	for (script, expected) in [
		("exit 0", Exit::Code(0)),
		("exit 1", Exit::Code(1)),
		("exit 127", Exit::Code(127)),
		("exit 255", Exit::Code(255)),
		("kill -HUP $$", Exit::Signal(libc::SIGHUP)),
		("kill -KILL $$", Exit::Signal(libc::SIGKILL)),
	] {
		let (child, _pair) = start(script);
		assert_eq!(within(&watched(&reaper, child), 5), expected, "{script}");
	}
}

#[test]
fn more_children_ending_at_once_than_one_wait_takes_all_arrive() {
	let reaper = Reaper::new().unwrap();
	let children: Vec<_> = (0..40).map(|_| start("exit 0")).collect();
	thread::sleep(Duration::from_millis(300));
	let (send, ended) = mpsc::channel();
	let mut pairs = Vec::new();
	for (child, pair) in children {
		pairs.push(pair);
		let send = send.clone();
		reaper.watch(child, move |exit| {
			let _ = send.send(exit.unwrap());
		});
	}
	drop(send);
	let all: Vec<_> = (0..40).map(|_| within_ok(&ended)).collect();
	assert_eq!(all, vec![Exit::Code(0); 40]);
}

#[test]
fn a_shell_that_leaves_a_job_running_is_reaped_when_it_exits() {
	// Its background job still holds the terminal; the shell's end is
	// its own, not the job's.
	let reaper = Reaper::new().unwrap();
	let (child, _pair) = start("sleep 3 & exit 3");
	let started = std::time::Instant::now();
	assert_eq!(within(&watched(&reaper, child), 5), Exit::Code(3));
	assert!(
		started.elapsed() < Duration::from_secs(2),
		"waited for the job"
	);
}

#[test]
fn one_reaper_lasts_for_many_shells_over_time() {
	let reaper = Reaper::new().unwrap();
	for n in 0..30 {
		let (child, _pair) = start(&format!("exit {}", n % 4));
		assert_eq!(
			within(&watched(&reaper, child), 5),
			Exit::Code(n % 4),
			"shell {n}"
		);
	}
}
