//! A terminal shared between its parser thread and the UI thread.
//!
//! The parser thread holds the lock while applying a bounded chunk of
//! output; the UI thread holds it to read state or copy the visible rows
//! for a frame. `std::sync::Mutex` deadlocks if one thread locks twice, so
//! debug builds track the locks each thread holds and panic instead.
use std::{
	ops::{Deref, DerefMut},
	sync::{
		Mutex, MutexGuard,
		atomic::{AtomicUsize, Ordering},
	},
	thread,
};

use super::Term;

pub(crate) struct SharedTerm {
	term: Mutex<Term>,
	/// Threads blocked in [`lock`](Self::lock).
	waiting: AtomicUsize,
}

impl SharedTerm {
	pub(crate) fn new(term: Term) -> Self {
		Self {
			term: Mutex::new(term),
			waiting: AtomicUsize::new(0),
		}
	}

	/// Locks the terminal. Never call this while already holding the lock
	/// on the same thread; pass `&Term` to helpers instead.
	pub(crate) fn lock(&self) -> TermGuard<'_> {
		#[cfg(debug_assertions)]
		held::acquire(self);
		self.waiting.fetch_add(1, Ordering::AcqRel);
		let guard = self
			.term
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner());
		self.waiting.fetch_sub(1, Ordering::AcqRel);
		TermGuard {
			guard,
			#[cfg(debug_assertions)]
			owner: self,
		}
	}
}

impl SharedTerm {
	/// Lets threads waiting for the lock take it first. `std` mutexes are
	/// not fair: a thread that unlocks and relocks in a loop, like the
	/// parser between chunks, could otherwise keep the UI waiting.
	pub(crate) fn yield_to_waiters(&self) {
		while self.waiting.load(Ordering::Acquire) > 0 {
			thread::yield_now();
		}
	}
}

pub(crate) struct TermGuard<'a> {
	guard: MutexGuard<'a, Term>,
	#[cfg(debug_assertions)]
	owner: &'a SharedTerm,
}

impl Deref for TermGuard<'_> {
	type Target = Term;

	fn deref(&self) -> &Term {
		&self.guard
	}
}

impl DerefMut for TermGuard<'_> {
	fn deref_mut(&mut self) -> &mut Term {
		&mut self.guard
	}
}

impl Drop for TermGuard<'_> {
	fn drop(&mut self) {
		#[cfg(debug_assertions)]
		held::release(self.owner);
	}
}

/// Debug-only record of the terminals each thread has locked.
#[cfg(debug_assertions)]
mod held {
	use std::cell::RefCell;

	thread_local! {
		static HELD: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
	}

	pub(super) fn acquire(term: &super::SharedTerm) {
		let address = term as *const _ as usize;
		HELD.with_borrow_mut(|held| {
			assert!(
				!held.contains(&address),
				"terminal already locked by this thread (would deadlock)"
			);
			held.push(address);
		});
	}

	pub(super) fn release(term: &super::SharedTerm) {
		let address = term as *const _ as usize;
		HELD.with_borrow_mut(|held| held.retain(|&a| a != address));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::theme::{Theme, ThemeKind};
	use std::sync::Arc;

	fn shared() -> Arc<SharedTerm> {
		Arc::new(SharedTerm::new(Term::new(
			4,
			2,
			Theme::from_kind(ThemeKind::Poop),
			"/".into(),
		)))
	}

	#[test]
	fn another_thread_sees_every_history_row_whole_after_the_unlock() {
		let term = shared();
		let writer = {
			let term = term.clone();
			thread::spawn(move || {
				for n in 0..3000 {
					let line = format!("\r\n{n:04}");
					crate::parser::Parser::new().advance(&mut term.lock(), line.as_bytes());
				}
			})
		};
		// Rows are numbered in order, so a stale or torn row breaks the count.
		// Checks at least once more after the writer is done.
		loop {
			let done = writer.is_finished();
			let t = term.lock();
			let newest = t.scrollback.len().saturating_sub(64);
			let rows: Vec<String> = (newest..t.scrollback.len())
				.map(|i| t.scrollback[i].iter().map(|c| c.ch).collect())
				.collect();
			for pair in rows.windows(2).filter(|pair| !pair[0].is_empty()) {
				let number = |row: &String| row.parse::<u32>().unwrap();
				assert_eq!(number(&pair[1]), number(&pair[0]) + 1, "{pair:?}");
			}
			if done {
				break;
			}
		}
		writer.join().unwrap();
	}

	#[test]
	fn other_threads_see_updates() {
		let term = shared();
		let writer = term.clone();
		thread::spawn(move || writer.lock().title = "from parser".into())
			.join()
			.unwrap();
		assert_eq!(term.lock().title, "from parser");
	}

	fn history(term: &Term) -> Vec<String> {
		(0..term.scrollback.len())
			.map(|row| term.scrollback[row].iter().map(|c| c.ch).collect())
			.collect()
	}

	#[test]
	fn history_written_under_the_lock_is_read_intact_on_another_thread() {
		let term = shared();
		term.lock().scrollback_limit = 50;
		let writer = term.clone();
		thread::spawn(move || {
			let mut parser = crate::parser::Parser::new();
			for line in 0..200 {
				parser.advance(&mut writer.lock(), format!("{line:04}\r\n").as_bytes());
			}
		})
		.join()
		.unwrap();
		let expected: Vec<String> = (149..199).map(|line| format!("{line:04}")).collect();
		assert_eq!(history(&term.lock()), expected);
	}

	#[test]
	fn a_reader_never_sees_history_rows_half_written() {
		let term = shared();
		term.lock().scrollback_limit = 30;
		let writer = term.clone();
		let parser = thread::spawn(move || {
			let mut parser = crate::parser::Parser::new();
			for line in 0..3000 {
				parser.advance(&mut writer.lock(), format!("{line:04}\r\n").as_bytes());
			}
		});
		while !parser.is_finished() {
			let rows = history(&term.lock());
			// Lines are consecutive numbers, so every row follows the one before.
			let numbers: Vec<u32> = rows.iter().map(|row| row.parse().unwrap()).collect();
			assert!(
				numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
				"{rows:?}"
			);
		}
		parser.join().unwrap();
		assert_eq!(
			history(&term.lock()).last().map(String::as_str),
			Some("2998")
		);
	}

	#[test]
	fn a_lock_can_be_taken_again_after_it_is_released() {
		let term = shared();
		drop(term.lock());
		drop(term.lock());
	}

	#[test]
	fn a_waiting_thread_gets_the_lock_before_the_holder_relocks() {
		let term = shared();
		let guard = term.lock();
		let ui = {
			let term = term.clone();
			thread::spawn(move || term.lock().title = "ui".into())
		};
		while term.waiting.load(Ordering::Acquire) == 0 {
			thread::yield_now();
		}
		drop(guard);
		term.yield_to_waiters();
		assert_eq!(term.lock().title, "ui");
		ui.join().unwrap();
	}

	#[cfg(debug_assertions)]
	#[test]
	#[should_panic(expected = "already locked by this thread")]
	fn locking_twice_on_one_thread_panics_instead_of_deadlocking() {
		let term = shared();
		let _first = term.lock();
		let _second = term.lock();
	}
}
