use std::{
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
		mpsc,
	},
	thread,
	time::Duration,
};

use super::*;

#[test]
fn quieting_catches_every_job_signal_so_none_can_end_the_app() {
	quiet_job_signals();
	for signal in QUIETED {
		assert!(caught(signal), "signal {signal} still at its default");
	}
	quiet_job_signals();
	assert!(caught(libc::SIGINT), "quieting twice keeps the handler");
}

#[test]
fn an_uncaught_signal_is_never_sent() {
	// SIGUSR2's default ends the process; nothing here catches it.
	assert!(!caught(libc::SIGUSR2));
	assert!(
		!Thread::current().signal(libc::SIGUSR2),
		"refused, so the test lives"
	);
}

#[test]
fn a_caught_signal_reaches_the_thread_and_interrupts_its_sleep() {
	quiet_job_signals();
	let (sender, receiver) = mpsc::channel();
	let woke = Arc::new(AtomicBool::new(false));
	let flag = woke.clone();
	let sleeper = thread::spawn(move || {
		sender.send(Thread::current()).unwrap();
		// `sleep` would sleep on through a signal; `nanosleep` itself stops
		// early, which shows the signal landed on this thread.
		let request = libc::timespec {
			tv_sec: 10,
			tv_nsec: 0,
		};
		let mut left = libc::timespec {
			tv_sec: 0,
			tv_nsec: 0,
		};
		// SAFETY: both timespecs live on this stack through the call.
		let slept = unsafe { libc::nanosleep(&request, &mut left) };
		flag.store(slept == -1, Ordering::SeqCst);
	});
	let thread = receiver.recv().unwrap();
	thread::sleep(Duration::from_millis(50));
	assert!(thread.signal(libc::SIGWINCH));
	sleeper.join().unwrap();
	assert!(woke.load(Ordering::SeqCst), "nanosleep was interrupted");
}
