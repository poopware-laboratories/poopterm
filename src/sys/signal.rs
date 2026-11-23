//! Signals for programs run inside this process (`inproc`): `^C` reaches
//! the thread running one as `SIGINT`, which the command shim turns into
//! that program's end, and a resize as `SIGWINCH`. Signals whose default
//! would stop or end the whole app are caught by a handler that does
//! nothing, so one sent when no program is listening is harmless.

/// The signals a terminal sends a program, and `SIGALRM`, which programs
/// arm with `alarm`: by default each would end or stop the app.
const QUIETED: [libc::c_int; 6] = [
	libc::SIGINT,
	libc::SIGQUIT,
	libc::SIGTSTP,
	libc::SIGHUP,
	libc::SIGALRM,
	libc::SIGWINCH,
];

extern "C" fn nothing(_: libc::c_int) {}

/// Catches `QUIETED` with a handler that does nothing, where the signal is
/// still at its default; one ignored or caught already is left alone.
pub(crate) fn quiet_job_signals() {
	for signal in QUIETED {
		let mut current = std::mem::MaybeUninit::<libc::sigaction>::uninit();
		// SAFETY: with a null new action, `sigaction` only writes the current
		// one into `current`, which has room for it.
		if unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) } != 0 {
			continue;
		}
		// SAFETY: filled by the successful call above.
		let current = unsafe { current.assume_init() };
		if current.sa_sigaction != libc::SIG_DFL {
			continue;
		}
		let mut quiet = current;
		quiet.sa_sigaction = nothing as extern "C" fn(libc::c_int) as libc::sighandler_t;
		quiet.sa_flags = libc::SA_RESTART;
		// SAFETY: installs a handler that touches nothing, from an action
		// the system gave us with only the handler and flags changed.
		unsafe { libc::sigaction(signal, &quiet, std::ptr::null_mut()) };
	}
}

/// Whether `signal` is caught (not at its default, not ignored).
pub(crate) fn caught(signal: libc::c_int) -> bool {
	let mut current = std::mem::MaybeUninit::<libc::sigaction>::uninit();
	// SAFETY: as in `quiet_job_signals`: writes only into `current`.
	if unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) } != 0 {
		return false;
	}
	// SAFETY: filled by the successful call above.
	let handler = unsafe { current.assume_init() }.sa_sigaction;
	handler != libc::SIG_DFL && handler != libc::SIG_IGN
}

/// A thread of this process, to signal.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Thread(libc::pthread_t);

impl Thread {
	/// The calling thread.
	pub(crate) fn current() -> Self {
		// SAFETY: takes nothing and returns the calling thread's handle.
		Self(unsafe { libc::pthread_self() })
	}

	/// Sends `signal` to the thread, if the process catches it: one at its
	/// default would end the whole app. Whether it was sent.
	pub(crate) fn signal(self, signal: libc::c_int) -> bool {
		// SAFETY: the handle is a live thread's: a `Thread` is taken by the
		// thread running a program and signalled only while it runs one
		// (`inproc::terminal`), and the signal is caught, never default.
		caught(signal) && unsafe { libc::pthread_kill(self.0, signal) } == 0
	}
}

#[cfg(test)]
#[path = "signal_tests.rs"]
mod tests;
