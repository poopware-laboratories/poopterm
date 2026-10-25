//! The delegated cgroup subtree (Linux): in a cgroup that is ours to
//! manage, the terminal sits in a `terminal` leaf with a higher CPU weight
//! and each shell in a `tab-<pid>` leaf with a memory limit, following the
//! kernel's and systemd's cgroup delegation rules.
use std::{
	fs, io,
	path::{Component, Path, PathBuf},
	thread,
	time::Duration,
};

/// The terminal leaf's CPU weight, against the default 100 of each tab.
pub(crate) const TERMINAL_CPU_WEIGHT: u64 = 400;

/// The controllers the layout uses, when the parent delegates them.
const CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

/// How long to wait between looks for systemd having moved us.
const WAIT: Duration = Duration::from_millis(10);

/// The cgroup this process is in, under the cgroup2 `mount`, from
/// `/proc/self/cgroup`; `None` for the root, hybrid hierarchies and paths
/// that are not plain and absolute.
pub(crate) fn own_cgroup(mount: &Path, proc_self_cgroup: &str) -> Option<PathBuf> {
	let mut lines = proc_self_cgroup.lines();
	let (Some(line), None) = (lines.next(), lines.next()) else {
		return None;
	};
	let path = line.strip_prefix("0::/")?;
	let plain = path.split('/').all(|part| {
		!part.is_empty()
			&& matches!(
				Path::new(part).components().next(),
				Some(Component::Normal(_))
			)
	});
	plain.then(|| mount.join(path))
}

/// Builds the layout in the cgroup systemd made for us: waits until
/// `/proc/self/cgroup` (read with `read`) shows the `scope`, up to
/// `attempts` looks. Without a scope, or if we are never moved, uses our
/// own cgroup, but only when no other process shares it.
pub(crate) fn delegated(
	read: &impl Fn(&str) -> String,
	mount: &Path,
	pid: u32,
	memory_high: Option<u64>,
	scope: Option<&str>,
	attempts: usize,
) -> Option<Tree> {
	if let Some(scope) = scope {
		for attempt in 0..attempts {
			let own = own_cgroup(mount, &read("/proc/self/cgroup"));
			if let Some(dir) = own.filter(|dir| dir.ends_with(scope)) {
				return Tree::adopt(&dir, pid, false, memory_high);
			}
			if attempt + 1 < attempts {
				thread::sleep(WAIT);
			}
		}
	}
	let own = own_cgroup(mount, &read("/proc/self/cgroup"))?;
	Tree::adopt(&own, pid, true, memory_high)
}

/// A cgroup laid out as a `terminal` leaf and one leaf per tab.
#[derive(Debug)]
pub(crate) struct Tree {
	dir: PathBuf,
	memory_high: Option<u64>,
}

impl Tree {
	/// Lays out `dir`, which must hold the terminal process `pid`: every
	/// process in it moves into the `terminal` leaf, ours first, since a
	/// cgroup with processes cannot enable controllers for its children;
	/// then the delegated controllers are enabled. With `alone`, refuses a
	/// cgroup that any other process shares.
	pub(crate) fn adopt(dir: &Path, pid: u32, alone: bool, memory_high: Option<u64>) -> Option<Tree> {
		let procs = pids(&fs::read_to_string(dir.join("cgroup.procs")).ok()?)?;
		if !procs.contains(&pid) || (alone && procs.iter().any(|&other| other != pid)) {
			return None;
		}
		let terminal = dir.join("terminal");
		create(&terminal).ok()?;
		fs::write(terminal.join("cgroup.procs"), pid.to_string()).ok()?;
		for other in procs.into_iter().filter(|&other| other != pid) {
			let _ = fs::write(terminal.join("cgroup.procs"), other.to_string());
		}
		let offered = fs::read_to_string(dir.join("cgroup.controllers")).unwrap_or_default();
		let enable: Vec<String> = CONTROLLERS
			.into_iter()
			.filter(|name| offered.split_whitespace().any(|offer| offer == *name))
			.map(|name| format!("+{name}"))
			.collect();
		if !enable.is_empty() {
			let _ = fs::write(dir.join("cgroup.subtree_control"), enable.join(" "));
		}
		let tree = Tree {
			dir: dir.to_owned(),
			memory_high,
		};
		if tree.enabled("cpu") {
			let _ = fs::write(terminal.join("cpu.weight"), TERMINAL_CPU_WEIGHT.to_string());
		}
		Some(tree)
	}

	/// Moves the shell `pid` into a `tab-<pid>` leaf, limited to the memory
	/// limit, after removing the empty leaves of closed tabs. `false` when
	/// it could not be moved, such as when it was started outside the tree.
	pub(crate) fn place(&self, pid: u32) -> bool {
		let name = format!("tab-{pid}");
		self.prune(&name);
		let leaf = self.dir.join(&name);
		if create(&leaf).is_err() {
			return false;
		}
		if let Some(bytes) = self.memory_high.filter(|_| self.enabled("memory")) {
			let _ = fs::write(leaf.join("memory.high"), bytes.to_string());
		}
		fs::write(leaf.join("cgroup.procs"), pid.to_string()).is_ok()
	}

	/// Whether `controller` is enabled for the leaves.
	fn enabled(&self, controller: &str) -> bool {
		let text = fs::read_to_string(self.dir.join("cgroup.subtree_control")).unwrap_or_default();
		text
			.split_whitespace()
			.any(|name| name.trim_start_matches('+') == controller)
	}

	/// Removes the tab leaves other than `keep`; cgroupfs refuses to remove
	/// those that still hold processes, so only closed tabs' leaves go.
	fn prune(&self, keep: &str) {
		let Ok(entries) = fs::read_dir(&self.dir) else {
			return;
		};
		for entry in entries.flatten() {
			let name = entry.file_name();
			let name = name.to_string_lossy();
			if name.starts_with("tab-") && name != keep {
				let _ = fs::remove_dir(entry.path());
			}
		}
	}
}

/// The process ids in a `cgroup.procs` file, or `None` if one is not a number.
fn pids(text: &str) -> Option<Vec<u32>> {
	text.lines().map(|line| line.trim().parse().ok()).collect()
}

/// Creates the cgroup `dir`, or keeps it if it is already there.
fn create(dir: &Path) -> io::Result<()> {
	match fs::create_dir(dir) {
		Err(error) if error.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
		other => other,
	}
}

#[cfg(test)]
#[path = "cgroup_tree_tests.rs"]
mod tests;
