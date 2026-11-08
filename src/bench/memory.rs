//! The memory of the terminal `--terminal` measures: which process it is
//! (the first ancestor on another terminal than ours), what it holds
//! resident, and its peak where the system keeps one.

use std::process::Command;

/// Parent and controlling terminal of a process, as `ps` shows them.
pub(crate) type Stat = (u32, String);

/// How far up the process tree to look for the terminal.
const DEPTH: usize = 64;

/// `ps -o ppid=,tty=` output as a [`Stat`].
pub(crate) fn parse_stat(text: &str) -> Option<Stat> {
	let mut fields = text.split_whitespace();
	let (parent, tty) = (fields.next()?.parse().ok()?, fields.next()?);
	fields.next().is_none().then(|| (parent, tty.to_owned()))
}

/// The first ancestor of `pid` not attached to `pid`'s terminal: the
/// emulator serving it. `None` if the chain ends or `stat` fails first.
pub(crate) fn terminal_pid(mut pid: u32, stat: impl Fn(u32) -> Option<Stat>) -> Option<u32> {
	let (_, ours) = stat(pid)?;
	for _ in 0..DEPTH {
		let (parent, tty) = stat(pid)?;
		if tty != ours {
			return Some(pid);
		}
		pid = parent;
	}
	None
}

/// The peak resident KiB in a `/proc/<pid>/status` file (`VmHWM`).
pub(crate) fn peak_kib(status: &str) -> Option<u64> {
	let line = status
		.lines()
		.find_map(|line| line.strip_prefix("VmHWM:"))?;
	line.split_whitespace().next()?.parse().ok()
}

/// `ps -o FIELD= -p pid`, if the process exists.
fn ps(pid: u32, field: &str) -> Option<String> {
	let pid = pid.to_string();
	let run = Command::new("ps")
		.args(["-o", field, "-p", &pid])
		.output()
		.ok()?;
	run
		.status
		.success()
		.then(|| String::from_utf8_lossy(&run.stdout).into_owned())
}

/// `pid`'s [`Stat`], from `ps`.
pub(crate) fn stat(pid: u32) -> Option<Stat> {
	parse_stat(&ps(pid, "ppid=,tty=")?)
}

/// What `pid` holds resident now, in KiB.
pub(crate) fn resident_kib(pid: u32) -> Option<u64> {
	ps(pid, "rss=")?.trim().parse().ok()
}

/// `pid`'s peak resident KiB, where the system keeps it (Linux).
pub(crate) fn peak(pid: u32) -> Option<u64> {
	peak_kib(&std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?)
}

/// The memory note: `pid`'s resident MiB `before` and `after` the run,
/// and at its peak when known.
pub(crate) fn note(pid: u32, before: Option<u64>, after: Option<u64>, peak: Option<u64>) -> String {
	let mib = |kib: Option<u64>| kib.map_or("-".into(), |kib| format!("{:.1}", kib as f64 / 1024.0));
	let (before, after, peak) = (mib(before), mib(after), mib(peak));
	format!("memory of pid {pid}: {before} MiB at start, {after} MiB at the end, {peak} MiB at peak")
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashMap;

	#[test]
	fn a_stat_is_the_parent_and_the_terminal() {
		assert_eq!(parse_stat("  412 pts/3\n"), Some((412, "pts/3".into())));
		assert_eq!(parse_stat("1 ?"), Some((1, "?".into())));
		// macOS names terminals ttys001 and shows `??` for none.
		assert_eq!(parse_stat(" 98 ttys001"), Some((98, "ttys001".into())));
	}

	#[test]
	fn anything_else_is_no_stat() {
		assert_eq!(parse_stat(""), None);
		assert_eq!(parse_stat("412"), None);
		assert_eq!(parse_stat("x pts/3"), None);
		assert_eq!(parse_stat("1 pts/3 extra"), None);
	}

	/// A process table: pid → (parent, terminal).
	fn table(rows: &[(u32, u32, &str)]) -> impl Fn(u32) -> Option<Stat> {
		let rows: HashMap<u32, Stat> = (rows.iter())
			.map(|&(pid, parent, tty)| (pid, (parent, tty.to_owned())))
			.collect();
		move |pid| rows.get(&pid).cloned()
	}

	#[test]
	fn the_terminal_is_the_first_ancestor_off_our_terminal() {
		// benchmark 30 ← shell 20 (both on pts/3) ← emulator 10 ← init 1.
		let stat = table(&[
			(30, 20, "pts/3"),
			(20, 10, "pts/3"),
			(10, 1, "?"),
			(1, 0, "?"),
		]);
		assert_eq!(terminal_pid(30, &stat), Some(10));
		assert_eq!(terminal_pid(20, &stat), Some(10));
	}

	#[test]
	fn under_a_multiplexer_the_terminal_is_the_multiplexer() {
		// benchmark 40 ← shell 30 on tmux's pts/7 ← tmux server 20, which has
		// no terminal of its own, though its client runs in the emulator.
		let stat = table(&[(40, 30, "pts/7"), (30, 20, "pts/7"), (20, 1, "?")]);
		assert_eq!(terminal_pid(40, stat), Some(20));
	}

	#[test]
	fn a_broken_chain_finds_no_terminal() {
		// The parent exited, or nothing above is on another terminal.
		assert_eq!(terminal_pid(30, table(&[(30, 20, "pts/3")])), None);
		let loop_ = table(&[(30, 20, "pts/3"), (20, 30, "pts/3")]);
		assert_eq!(terminal_pid(30, loop_), None);
		assert_eq!(terminal_pid(5, table(&[])), None);
	}

	#[test]
	fn the_peak_comes_from_vm_hwm() {
		let status = "Name:\tpoopterm\nVmPeak:\t  900000 kB\nVmHWM:\t   81234 kB\nVmRSS:\t 70000 kB\n";
		assert_eq!(peak_kib(status), Some(81234));
		assert_eq!(peak_kib("Name:\tx\nVmRSS:\t1 kB\n"), None);
		assert_eq!(peak_kib("VmHWM:\tlots kB\n"), None);
	}

	#[test]
	fn this_process_has_a_stat_and_resident_memory() {
		let pid = std::process::id();
		assert!(stat(pid).is_some_and(|(parent, _)| parent > 0));
		// KiB, not bytes or pages: a test binary holds a few MiB, not TiB.
		assert!(resident_kib(pid).is_some_and(|kib| (1024..1 << 24).contains(&kib)));
		assert_eq!(resident_kib(u32::MAX), None);
	}

	#[test]
	fn the_note_shows_mib_and_dashes_for_what_is_unknown() {
		let note = note(42, Some(51_200), Some(76_800), Some(102_400));
		assert_eq!(
			note,
			"memory of pid 42: 50.0 MiB at start, 75.0 MiB at the end, 100.0 MiB at peak"
		);
		let note = super::note(7, None, Some(1024), None);
		assert_eq!(
			note,
			"memory of pid 7: - MiB at start, 1.0 MiB at the end, - MiB at peak"
		);
	}

	/// A chain of shells on pts/1 from pid 1000 up, with the emulator
	/// `depth` steps above the start.
	fn chain(depth: u32) -> impl Fn(u32) -> Option<Stat> {
		move |pid| match pid - 1000 {
			step if step < depth => Some((pid + 1, "pts/1".into())),
			_ => Some((1, "?".into())),
		}
	}

	#[test]
	fn the_search_stops_after_depth_steps() {
		let last = DEPTH as u32 - 1;
		assert_eq!(terminal_pid(1000, chain(last)), Some(1000 + last));
		assert_eq!(terminal_pid(1000, chain(last + 1)), None);
	}

	#[test]
	fn negative_or_huge_parents_are_no_stat() {
		assert_eq!(parse_stat("-1 pts/3"), None);
		assert_eq!(parse_stat("4294967296 pts/3"), None);
		assert_eq!(
			parse_stat("4294967295 pts/3"),
			Some((u32::MAX, "pts/3".into()))
		);
	}

	#[test]
	fn mib_are_rounded_to_a_tenth() {
		let note = note(1, Some(1536), Some(1), Some(1075));
		assert_eq!(
			note,
			"memory of pid 1: 1.5 MiB at start, 0.0 MiB at the end, 1.0 MiB at peak"
		);
	}

	#[test]
	fn a_process_that_does_not_exist_has_no_stat_or_peak() {
		assert_eq!(stat(u32::MAX), None);
		assert_eq!(peak(u32::MAX), None);
	}

	#[cfg(target_os = "linux")]
	#[test]
	fn this_process_has_a_peak_at_least_what_it_holds() {
		let pid = std::process::id();
		// Read in this order, the peak so far is at least what was held.
		let now = resident_kib(pid).unwrap();
		assert!(peak(pid).unwrap() >= now, "{now}");
	}
}
