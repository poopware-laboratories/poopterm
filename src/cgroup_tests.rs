//! Per-shell cgroups: detecting the unified hierarchy, the memory limit,
//! the systemd request, and the worker against a fake systemd on a
//! private bus.
use std::{io, path::Path, sync::mpsc, thread, time::Duration};

use super::{
	NO_AUTO_START, NO_REPLY_EXPECTED, available, memory_high, place, run, start, start_scope, unified,
};
use crate::dbus::{
	Connection,
	message::{Kind, Message},
	test_bus::TestBus,
	wire::Value,
};

const SYSTEMD: &str = "org.freedesktop.systemd1";

#[test]
fn only_the_unified_hierarchy_counts() {
	assert!(unified("0::/user.slice/user-1000.slice/session-c1.scope\n"));
	assert!(unified("0::/"), "without a final newline");
	// Hybrid: v1 controllers mounted beside the unified tree.
	let hybrid = "12:pids:/user.slice\n1:name=systemd:/user.slice\n0::/user.slice\n";
	assert!(!unified(hybrid));
	assert!(!unified("1:name=systemd:/user.slice\n"), "v1 only");
	assert!(!unified(""), "no cgroups at all");
	assert!(!unified("0:cpu:/"), "a v1 line numbered 0");
	assert!(!unified("0::/a\n0::/b\n"), "two unified lines");
}

#[test]
fn availability_needs_cgroup2_mounted_too() {
	let ours = "0::/user.slice\n";
	assert!(available(ours, true));
	// No `cgroup.controllers`: /sys/fs/cgroup is missing or v1.
	assert!(!available(ours, false));
	assert!(!available("1:name=systemd:/\n", true));
}

#[test]
fn the_memory_limit_is_seven_eighths_of_ram() {
	let meminfo = "MemTotal:       16000000 kB\nMemFree:         1000 kB\n";
	assert_eq!(memory_high(meminfo), Some(16_000_000 * 1024 / 8 * 7));
	let later = "MemFree:         1000 kB\nMemTotal: 8 kB\n";
	assert_eq!(memory_high(later), Some(7168), "found on any line");
}

#[test]
fn an_unreadable_memory_total_sets_no_limit() {
	for meminfo in [
		"",
		"MemFree: 1000 kB\n",
		"MemTotal: lots kB\n",
		"MemTotal: 0 kB\n",
		"MemTotal: 18446744073709551615 kB\n",
		"MemTotalish: 1000 kB\n",
		"MemTotal: 1000\n",
		"MemTotal: 1000 MB\n",
		"MemTotal: -1 kB\n",
	] {
		assert_eq!(memory_high(meminfo), None, "{meminfo:?}");
	}
}

/// The properties of a `StartTransientUnit` body, by name.
fn properties(body: &[Value]) -> Vec<(String, Value)> {
	let Some(Value::Array(signature, entries)) = body.get(2) else {
		panic!("no properties: {body:?}")
	};
	assert_eq!(signature, "(sv)");
	entries
		.iter()
		.map(|entry| match entry {
			Value::Struct(fields) => match &fields[..] {
				[Value::Str(name), Value::Variant(value)] => (name.clone(), (**value).clone()),
				other => panic!("not a property: {other:?}"),
			},
			other => panic!("not a struct: {other:?}"),
		})
		.collect()
}

fn property(body: &[Value], name: &str) -> Option<Value> {
	properties(body)
		.into_iter()
		.find(|(key, _)| key == name)
		.map(|(_, value)| value)
}

#[test]
fn the_request_names_a_scope_for_the_shell() {
	let message = start_scope(4242, Some(1 << 30));
	assert_eq!(message.destination.as_deref(), Some(SYSTEMD));
	assert_eq!(message.path.as_deref(), Some("/org/freedesktop/systemd1"));
	assert_eq!(
		message.interface.as_deref(),
		Some("org.freedesktop.systemd1.Manager")
	);
	assert_eq!(message.member.as_deref(), Some("StartTransientUnit"));
	let body = &message.body;
	assert_eq!(body[0], Value::Str("app-poopterm-4242.scope".into()));
	assert_eq!(body[1], Value::Str("fail".into()));
	assert_eq!(body[3], Value::Array("(sa(sv))".into(), Vec::new()));
	let signature: String = body.iter().map(Value::signature).collect();
	assert_eq!(signature, "ssa(sv)a(sa(sv))");
}

#[test]
fn the_scope_holds_the_shell_with_accounting_and_a_memory_limit() {
	let body = start_scope(4242, Some(1 << 30)).body;
	let pids = Value::Array("u".into(), vec![Value::U32(4242)]);
	assert_eq!(property(&body, "PIDs"), Some(pids));
	assert_eq!(property(&body, "MemoryAccounting"), Some(Value::Bool(true)));
	assert_eq!(property(&body, "TasksAccounting"), Some(Value::Bool(true)));
	assert_eq!(property(&body, "MemoryHigh"), Some(Value::U64(1 << 30)));
	// Collected when the shell exits, even if it failed.
	let collect = Some(Value::Str("inactive-or-failed".into()));
	assert_eq!(property(&body, "CollectMode"), collect);
	assert!(
		matches!(property(&body, "Description"), Some(Value::Str(text)) if text.contains("oopterm"))
	);
	assert_eq!(property(&start_scope(1, None).body, "MemoryHigh"), None);
}

#[test]
fn the_request_expects_no_reply_and_starts_no_service() {
	let message = start_scope(7, None);
	assert_eq!(message.flags, NO_REPLY_EXPECTED | NO_AUTO_START);
	assert_eq!(
		(NO_REPLY_EXPECTED, NO_AUTO_START),
		(1, 2),
		"the specification's bits"
	);
	// It survives the wire unchanged.
	let mut sent = message.clone();
	sent.serial = 3;
	assert_eq!(Message::decode(&sent.encode()), Ok(sent));
}

/// A fake systemd on `bus`, returning the receiving end of its connection.
fn fake_systemd(bus: &TestBus) -> crate::dbus::Receiver {
	let mut server = Connection::connect(&bus.address, None).unwrap();
	let mut request = Message::method_call(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		"org.freedesktop.DBus",
		"RequestName",
	);
	request.body = vec![Value::Str(SYSTEMD.into()), Value::U32(0)];
	server.call(request).unwrap();
	let (_, receiver) = server.split();
	receiver.set_timeout(Duration::from_secs(5));
	receiver
}

/// The next `StartTransientUnit` call `receiver` gets.
fn next_unit(receiver: &mut crate::dbus::Receiver) -> Message {
	loop {
		let message = receiver.receive().unwrap();
		if message.kind == Kind::MethodCall && message.member.as_deref() == Some("StartTransientUnit") {
			return message;
		}
	}
}

#[test]
fn each_shell_reaches_systemd_in_order() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut systemd = fake_systemd(&bus);
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			Some(4096),
		)
	});
	pids.send(100).unwrap();
	pids.send(200).unwrap();
	let first = next_unit(&mut systemd);
	assert_eq!(first.body[0], Value::Str("app-poopterm-100.scope".into()));
	assert_eq!(property(&first.body, "MemoryHigh"), Some(Value::U64(4096)));
	assert_eq!(first.flags, NO_REPLY_EXPECTED | NO_AUTO_START);
	let second = next_unit(&mut systemd);
	assert_eq!(second.body[0], Value::Str("app-poopterm-200.scope".into()));
	// The worker ends when the terminal stops sending shells.
	drop(pids);
	worker.join().unwrap();
}

#[test]
fn without_systemd_on_the_bus_the_worker_carries_on() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let worker =
		thread::spawn(move || run(receiver, true, || Connection::connect(&address, None), None));
	// Nobody owns the name: the bus drops the calls without an error reply.
	pids.send(1).unwrap();
	pids.send(2).unwrap();
	thread::sleep(Duration::from_millis(200));
	// Then systemd appears, and later shells reach it. A slow worker may
	// still deliver the first ones; what counts is that it did not stop.
	let mut systemd = fake_systemd(&bus);
	pids.send(3).unwrap();
	let three = Value::Str("app-poopterm-3.scope".into());
	while next_unit(&mut systemd).body[0] != three {}
	drop(pids);
	worker.join().unwrap();
}

#[test]
fn without_cgroup2_the_bus_is_never_opened() {
	let (pids, receiver) = mpsc::channel();
	let worker = thread::spawn(move || {
		run(
			receiver,
			false,
			|| -> io::Result<Connection> { panic!("connected") },
			None,
		)
	});
	worker.join().unwrap();
	// Later shells are dropped quietly.
	assert!(pids.send(1).is_err());
}

#[test]
fn without_a_bus_shells_are_dropped_quietly() {
	let (pids, receiver) = mpsc::channel();
	let worker =
		thread::spawn(move || run(receiver, true, || Err(io::ErrorKind::NotFound.into()), None));
	worker.join().unwrap();
	assert!(pids.send(1).is_err());
}

/// The system files the worker reads, as `start` sees them.
fn system(cgroup: &'static str, meminfo: &'static str) -> impl Fn(&str) -> String {
	move |path| match path {
		"/proc/self/cgroup" => cgroup.into(),
		"/proc/meminfo" => meminfo.into(),
		other => panic!("read {other}"),
	}
}

#[test]
fn the_worker_reads_the_system_files_then_places_shells() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut systemd = fake_systemd(&bus);
	let address = bus.address.clone();
	let pids = start(
		system("0::/user.slice\n", "MemTotal: 8 kB\n"),
		|path| path == "/sys/fs/cgroup/cgroup.controllers",
		move || Connection::connect(&address, None),
	);
	pids.send(55).unwrap();
	let message = next_unit(&mut systemd);
	assert_eq!(message.body[0], Value::Str("app-poopterm-55.scope".into()));
	assert_eq!(
		property(&message.body, "MemoryHigh"),
		Some(Value::U64(7168))
	);
}

#[test]
fn the_worker_stops_on_hybrid_or_unmounted_hierarchies() {
	for (cgroup, mounted) in [("1:name=systemd:/\n0::/\n", true), ("0::/\n", false)] {
		let pids = start(
			system(cgroup, ""),
			move |_| mounted,
			|| -> io::Result<Connection> { panic!("connected") },
		);
		// The worker drops its end once it gives up.
		let gone = (0..500).any(|_| {
			thread::sleep(Duration::from_millis(10));
			pids.send(1).is_err()
		});
		assert!(gone, "{cgroup:?} {mounted}");
	}
}

/// Whether this machine runs a systemd user manager we may talk to.
fn user_systemd() -> bool {
	let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
	available(
		&std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default(),
		true,
	) && Path::new(&runtime).join("systemd/private").exists()
		&& Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
}

#[test]
fn a_real_user_systemd_moves_the_shell_into_its_scope() {
	if !user_systemd() {
		return;
	}
	let mut child = std::process::Command::new("sleep")
		.arg("30")
		.spawn()
		.unwrap();
	place(child.id());
	let scope = format!("app-poopterm-{}.scope", child.id());
	let path = format!("/proc/{}/cgroup", child.id());
	let moved = (0..300).any(|_| {
		thread::sleep(Duration::from_millis(10));
		std::fs::read_to_string(&path).is_ok_and(|text| text.trim_end().ends_with(&scope))
	});
	let _ = child.kill();
	let _ = child.wait();
	assert!(moved, "{scope} not in {path}");
}

#[test]
fn the_memory_limit_never_overflows() {
	// The largest total whose bytes fit: multiplying by seven first would
	// overflow, dividing first does not.
	let kib = u64::MAX / 1024;
	let limit = memory_high(&format!("MemTotal: {kib} kB\n")).unwrap();
	assert_eq!(limit, kib * 1024 / 8 * 7);
	assert_eq!(memory_high("MemTotal: 1 kB\n"), Some(896));
	assert_eq!(memory_high(&format!("MemTotal: {} kB\n", kib + 1)), None);
}

#[test]
fn the_scope_sets_exactly_these_properties() {
	let names = |body: &[Value]| -> Vec<String> {
		properties(body).into_iter().map(|(name, _)| name).collect()
	};
	let base = [
		"Description",
		"PIDs",
		"CollectMode",
		"MemoryAccounting",
		"TasksAccounting",
	];
	assert_eq!(names(&start_scope(9, None).body), base);
	let mut limited = base.to_vec();
	limited.push("MemoryHigh");
	assert_eq!(names(&start_scope(9, Some(1)).body), limited);
	// The largest process id still names a valid unit.
	let name = start_scope(u32::MAX, None).body[0].clone();
	assert_eq!(name, Value::Str("app-poopterm-4294967295.scope".into()));
}

#[test]
fn a_bus_that_goes_away_drops_later_shells_quietly() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut systemd = fake_systemd(&bus);
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let worker =
		thread::spawn(move || run(receiver, true, || Connection::connect(&address, None), None));
	pids.send(1).unwrap();
	next_unit(&mut systemd);
	drop((systemd, bus));
	// Writes now fail; the worker neither panics nor blocks the sender.
	for pid in 2..50 {
		pids.send(pid).unwrap();
	}
	drop(pids);
	worker.join().unwrap();
}

#[test]
fn a_blank_or_indented_line_is_not_the_unified_hierarchy() {
	assert!(!unified("\n0::/\n"));
	assert!(!unified(" 0::/\n"));
	assert!(unified("0::/\n"));
}
