//! Each shell in its own cgroup (Linux), so one tab's runaway program is
//! accounted and throttled apart from the terminal and the other tabs.
//! systemd delegates a scope to the terminal over the session bus, which
//! `cgroup_tree` lays out as a `terminal` leaf with a higher CPU weight,
//! so a flooding tab cannot slow its UI and parser threads, and one leaf
//! per tab. Without systemd, our own cgroup is used when it is delegated
//! and ours alone; a shell no leaf can take gets a systemd scope of its
//! own. Without cgroup2, shells run as before.
use std::{
	fs, io,
	path::{Path, PathBuf},
	sync::{OnceLock, mpsc},
	thread,
};

use crate::{
	cgroup_tree::{Tree, delegated},
	dbus::{Connection, message::Message, wire::Value},
};

/// Where the delegated tree is looked for, and how many times, 10 ms
/// apart, to look for systemd having moved the terminal into its scope.
struct Delegation {
	mount: PathBuf,
	attempts: usize,
}

/// Message flags: the bus sends no reply, and starts no service for it.
const NO_REPLY_EXPECTED: u8 = 1;
const NO_AUTO_START: u8 = 2;

/// Moves the shell `pid` into a scope of its own, in the background.
pub(crate) fn place(pid: u32) {
	static WORKER: OnceLock<mpsc::Sender<u32>> = OnceLock::new();
	let worker = WORKER.get_or_init(|| {
		start(
			|path| fs::read_to_string(path).unwrap_or_default(),
			|path| Path::new(path).exists(),
			Connection::session,
			std::process::id(),
			Delegation {
				mount: "/sys/fs/cgroup".into(),
				attempts: 200,
			},
		)
	});
	let _ = worker.send(pid);
}

/// Starts the worker thread, which reads the system's cgroup and memory
/// files with `read` and `exists` before connecting with `connect`, and
/// lays out the `terminal` process's cgroup first, per `delegation`.
fn start(
	read: impl Fn(&str) -> String + Send + 'static,
	exists: impl Fn(&str) -> bool + Send + 'static,
	connect: impl FnOnce() -> io::Result<Connection> + Send + 'static,
	terminal: u32,
	delegation: Delegation,
) -> mpsc::Sender<u32> {
	let (pids, receiver) = mpsc::channel();
	let _ = thread::Builder::new()
		.name("cgroups".into())
		.spawn(move || {
			let cgroups = available(
				&read("/proc/self/cgroup"),
				exists("/sys/fs/cgroup/cgroup.controllers"),
			);
			let memory_high = memory_high(&read("/proc/meminfo"));
			let scope = terminal_scope_name(terminal);
			run(
				receiver,
				cgroups,
				connect,
				memory_high,
				Some(terminal),
				|asked| {
					let scope = asked.then_some(scope.as_str());
					let Delegation { mount, attempts } = delegation;
					delegated(&read, &mount, terminal, memory_high, scope, attempts)
				},
			)
		});
	pids
}

/// Asks systemd for a delegated scope for the `terminal` process and gets
/// its tree from `delegate`, told whether systemd was asked; then moves
/// each shell from `pids` into a leaf of the tree, or else asks systemd
/// for a scope for it, until they stop. Returns at once, dropping them,
/// when cgroups are missing, or both the tree and the bus are.
fn run(
	pids: mpsc::Receiver<u32>,
	available: bool,
	connect: impl FnOnce() -> io::Result<Connection>,
	memory_high: Option<u64>,
	terminal: Option<u32>,
	delegate: impl FnOnce(bool) -> Option<Tree>,
) {
	if !available {
		return;
	}
	let mut bus = connect().ok().map(Connection::split);
	let asked = match (&mut bus, terminal) {
		(Some((sender, _)), Some(pid)) => sender.send(terminal_scope(pid)).is_ok(),
		_ => false,
	};
	let tree = terminal.and_then(|_| delegate(asked));
	if bus.is_none() && tree.is_none() {
		return;
	}
	for pid in pids {
		if tree.as_ref().is_some_and(|tree| tree.place(pid)) {
			continue;
		}
		if let Some((sender, _)) = &mut bus {
			let _ = sender.send(start_scope(pid, memory_high));
		}
	}
}

/// Whether this process sits in the unified cgroup2 hierarchy alone, per
/// `/proc/self/cgroup`, and cgroup2 is mounted where systemd expects it.
fn available(proc_self_cgroup: &str, controllers_exist: bool) -> bool {
	unified(proc_self_cgroup) && controllers_exist
}

/// Whether `/proc/self/cgroup` lists only the unified hierarchy.
fn unified(proc_self_cgroup: &str) -> bool {
	let mut lines = proc_self_cgroup.lines();
	matches!((lines.next(), lines.next()), (Some(line), None) if line.starts_with("0::"))
}

/// The memory a shell's scope may use before it is throttled: seven
/// eighths of RAM, from `/proc/meminfo`, so a runaway tab slows down
/// before the OOM killer picks the terminal.
fn memory_high(meminfo: &str) -> Option<u64> {
	let line = meminfo
		.lines()
		.find_map(|line| line.strip_prefix("MemTotal:"))?;
	let kib: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
	let bytes = kib.checked_mul(1024).filter(|&bytes| bytes > 0)?;
	Some(bytes / 8 * 7)
}

/// The scope for the shell `pid`: accounted, and throttled past
/// `memory_high` bytes.
fn start_scope(pid: u32, memory_high: Option<u64>) -> Message {
	let mut properties = vec![
		("MemoryAccounting", Value::Bool(true)),
		("TasksAccounting", Value::Bool(true)),
	];
	properties.extend(memory_high.map(|bytes| ("MemoryHigh", Value::U64(bytes))));
	scope(
		format!("app-poopterm-{pid}.scope"),
		"Poopterm shell",
		pid,
		properties,
	)
}

/// The scope for the terminal process `pid`, delegated to us so that
/// `cgroup_tree` can lay it out.
fn terminal_scope(pid: u32) -> Message {
	let delegate = ("Delegate", Value::Bool(true));
	scope(terminal_scope_name(pid), "Poopterm", pid, vec![delegate])
}

fn terminal_scope_name(pid: u32) -> String {
	format!("app-poopterm-terminal-{pid}.scope")
}

/// The `StartTransientUnit` call making the scope `name` for `pid`, with
/// `extra` properties.
fn scope(name: String, description: &str, pid: u32, extra: Vec<(&str, Value)>) -> Message {
	let mut properties = vec![
		("Description", Value::Str(description.into())),
		("PIDs", Value::Array("u".into(), vec![Value::U32(pid)])),
		("CollectMode", Value::Str("inactive-or-failed".into())),
	];
	properties.extend(extra);
	let properties = properties
		.into_iter()
		.map(|(key, value)| {
			Value::Struct(vec![
				Value::Str(key.into()),
				Value::Variant(Box::new(value)),
			])
		})
		.collect();
	let mut message = Message::method_call(
		"org.freedesktop.systemd1",
		"/org/freedesktop/systemd1",
		"org.freedesktop.systemd1.Manager",
		"StartTransientUnit",
	);
	message.flags = NO_REPLY_EXPECTED | NO_AUTO_START;
	message.body = vec![
		Value::Str(name),
		Value::Str("fail".into()),
		Value::Array("(sv)".into(), properties),
		Value::Array("(sa(sv))".into(), Vec::new()),
	];
	message
}

#[cfg(test)]
#[path = "cgroup_tests.rs"]
mod tests;
