//! Warnings Poopterm prints to standard error, also kept in a bounded ring
//! for the Diagnostics page and its report. `warning!` does both.
use std::{
	collections::VecDeque,
	fmt,
	sync::{Mutex, MutexGuard, PoisonError},
	time::{Duration, SystemTime},
};

/// Warnings kept; older ones are dropped and counted.
pub(crate) const KEPT: usize = 256;
/// Longest kept line, in bytes, ellipsis included.
pub(crate) const LONGEST: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Warning {
	/// Time since the Unix epoch.
	pub(crate) at: Duration,
	pub(crate) text: String,
}

#[derive(Debug, Default)]
pub(crate) struct Ring {
	warnings: VecDeque<Warning>,
	dropped: usize,
}

impl Ring {
	const fn new() -> Self {
		Self {
			warnings: VecDeque::new(),
			dropped: 0,
		}
	}

	pub(crate) fn push(&mut self, at: Duration, text: &str) {
		if self.warnings.len() == KEPT {
			self.warnings.pop_front();
			self.dropped += 1;
		}
		self.warnings.push_back(Warning {
			at,
			text: clean(text),
		});
	}

	/// Oldest first.
	pub(crate) fn warnings(&self) -> impl Iterator<Item = &Warning> {
		self.warnings.iter()
	}

	pub(crate) fn dropped(&self) -> usize {
		self.dropped
	}
}

/// `text` fit for one line of the page and the report: control characters
/// replaced, cut to [`LONGEST`] bytes on a character boundary.
pub(crate) fn clean(text: &str) -> String {
	let mut line = String::new();
	for c in text.chars() {
		let c = if c.is_control() { '\u{fffd}' } else { c };
		if line.len() + c.len_utf8() > LONGEST {
			while line.len() + '…'.len_utf8() > LONGEST {
				line.pop();
			}
			line.push('…');
			break;
		}
		line.push(c);
	}
	line
}

static RING: Mutex<Ring> = Mutex::new(Ring::new());

fn ring() -> MutexGuard<'static, Ring> {
	RING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Prints `message` to standard error and keeps it in the ring.
pub(crate) fn warn(message: fmt::Arguments) {
	let text = message.to_string();
	eprintln!("poopterm: {text}");
	let at = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
	ring().push(at.unwrap_or_default(), &text);
}

/// A copy of the kept warnings, oldest first, and how many were dropped.
pub(crate) fn recent() -> (Vec<Warning>, usize) {
	let ring = ring();
	(ring.warnings().cloned().collect(), ring.dropped())
}

/// `eprintln!` with the `poopterm: ` prefix, also kept for Diagnostics.
macro_rules! warning {
	($($argument:tt)*) => {
		$crate::warnings::warn(format_args!($($argument)*))
	};
}
pub(crate) use warning;
