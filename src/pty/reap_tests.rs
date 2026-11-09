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
		let _ = send.send(thread::current().id());
	});
	let id = ran_on.recv_timeout(Duration::from_secs(5)).unwrap();
	assert_ne!(id, thread::current().id());
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
	// SAFETY: kills and reaps our own child behind the reaper's back.
	unsafe {
		libc::kill(pid, libc::SIGKILL);
		libc::waitpid(pid, std::ptr::null_mut(), 0);
	}
	reaper.watch(child, move |exit| {
		let _ = send.send(exit);
	});
	let exit = ended
		.recv_timeout(Duration::from_secs(5))
		.expect("never reported");
	assert!(exit.is_err(), "{exit:?}");
}
