//! Each shell in its own cgroup (Linux): systemd makes a transient scope
//! per shell on the session bus, so one tab's runaway program is
//! accounted and throttled apart from the terminal and the other tabs.
//! The terminal gets a scope too, beside them and with a higher CPU
//! weight, so a flooding tab cannot slow its UI and parser threads.
//! Without cgroup2, a session bus or systemd, shells run as before.
use std::{
	fs, io,
	path::Path,
	sync::{OnceLock, mpsc},
	thread,
};

use crate::dbus::{Connection, message::Message, wire::Value};

/// The terminal's CPU weight, against the default 100 of each shell.
const TERMINAL_CPU_WEIGHT: u64 = 400;

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
		)
	});
	let _ = worker.send(pid);
}

/// Starts the worker thread, which reads the system's cgroup and memory
/// files with `read` and `exists` before connecting with `connect`, and
/// moves the `terminal` process first.
fn start(
	read: impl Fn(&str) -> String + Send + 'static,
	exists: impl Fn(&str) -> bool + Send + 'static,
	connect: impl FnOnce() -> io::Result<Connection> + Send + 'static,
	terminal: u32,
) -> mpsc::Sender<u32> {
	let (pids, receiver) = mpsc::channel();
	let _ = thread::Builder::new()
		.name("cgroups".into())
		.spawn(move || {
			let cgroups = available(
				&read("/proc/self/cgroup"),
				exists("/sys/fs/cgroup/cgroup.controllers"),
			);
			run(
				receiver,
				cgroups,
				connect,
				memory_high(&read("/proc/meminfo")),
				Some(terminal),
			)
		});
	pids
}

/// Asks systemd for a scope for the `terminal` process, then for each
/// shell from `pids`, until they stop; returns at once, dropping them,
/// when cgroups or the bus are missing.
fn run(
	pids: mpsc::Receiver<u32>,
	available: bool,
	connect: impl FnOnce() -> io::Result<Connection>,
	memory_high: Option<u64>,
	terminal: Option<u32>,
) {
	let Some(connection) = available.then(connect).and_then(Result::ok) else {
		return;
	};
	let (mut sender, _replies) = connection.split();
	if let Some(pid) = terminal {
		let _ = sender.send(terminal_scope(pid));
	}
	for pid in pids {
		let _ = sender.send(start_scope(pid, memory_high));
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

/// The scope for the terminal process `pid`, with the higher CPU weight.
fn terminal_scope(pid: u32) -> Message {
	let weight = ("CPUWeight", Value::U64(TERMINAL_CPU_WEIGHT));
	scope(
		format!("app-poopterm-terminal-{pid}.scope"),
		"Poopterm",
		pid,
		vec![weight],
	)
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
