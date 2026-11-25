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

/// What `signal` does now: `SIG_DFL`, `SIG_IGN` or a handler.
fn handler(signal: libc::c_int) -> libc::sighandler_t {
	let mut current = std::mem::MaybeUninit::<libc::sigaction>::uninit();
	// SAFETY: with a null new action, `sigaction` only writes the current one
	// into `current`, which has room for it.
	let read = unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) };
	assert_eq!(read, 0);
	// SAFETY: filled by the successful call above.
	unsafe { current.assume_init() }.sa_sigaction
}

#[test]
fn quieting_leaves_no_job_signal_able_to_end_the_app() {
	quiet_job_signals();
	for signal in QUIETED {
		// One the test runner was started ignoring (a CI shell without job
		// control) stays ignored, which ends nothing either.
		assert_ne!(handler(signal), libc::SIG_DFL, "signal {signal}");
	}
	quiet_job_signals();
	assert_ne!(
		handler(libc::SIGINT),
		libc::SIG_DFL,
		"quieting twice keeps what was there"
	);
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
