//! Per-shell cgroups: detecting the unified hierarchy, the memory limit,
//! the systemd request, and the worker against a fake systemd on a
//! private bus.
use std::{io, path::Path, sync::mpsc, thread, time::Duration};

use super::{
	Delegation, NO_AUTO_START, NO_REPLY_EXPECTED, available, memory_high, place, run, start,
	start_scope, terminal_scope, unified,
};
use crate::cgroup_tree::Tree;
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
			None,
			|_| None,
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
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			None,
			None,
			|_| None,
		)
	});
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
			Some(1),
			|_| None,
		)
	});
	worker.join().unwrap();
	// Later shells are dropped quietly.
	assert!(pids.send(1).is_err());
}

#[test]
fn without_a_bus_shells_are_dropped_quietly() {
	let (pids, receiver) = mpsc::channel();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Err(io::ErrorKind::NotFound.into()),
			None,
			Some(1),
			|_| None,
		)
	});
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
		77,
		nowhere(),
	);
	pids.send(55).unwrap();
	let terminal = next_unit(&mut systemd);
	let name = Value::Str("app-poopterm-terminal-77.scope".into());
	assert_eq!(terminal.body[0], name, "the terminal first");
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
			1,
			nowhere(),
		);
		// The worker drops its end once it gives up.
		let gone = (0..500).any(|_| {
			thread::sleep(Duration::from_millis(10));
			pids.send(1).is_err()
		});
		assert!(gone, "{cgroup:?} {mounted}");
	}
}

/// Whether this machine runs a systemd user manager that may move us: we
/// must be in our user's slice, as in a login session, not in a system
/// service such as a CI runner's, which only the system manager may move.
fn user_systemd() -> bool {
	let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
	let ours = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
	// SAFETY: getuid takes no arguments and cannot fail.
	let slice = format!("/user-{}.slice/", unsafe { libc::getuid() });
	available(&ours, true)
		&& ours.contains(&slice)
		&& Path::new(&runtime).join("systemd/private").exists()
		&& Path::new("/sys/fs/cgroup/cgroup.controllers").exists()
}

/// Waits until `pid` is in a cgroup ending with `suffix`.
fn moved(pid: u32, suffix: &str) -> bool {
	let path = format!("/proc/{pid}/cgroup");
	(0..300).any(|_| {
		thread::sleep(Duration::from_millis(10));
		std::fs::read_to_string(&path).is_ok_and(|text| text.trim_end().ends_with(suffix))
	})
}

#[test]
fn a_real_user_systemd_delegates_a_scope_laid_out_per_tab() {
	if !user_systemd() {
		return;
	}
	let sleep = || {
		std::process::Command::new("sleep")
			.arg("30")
			.spawn()
			.unwrap()
	};
	// Started before the terminal moved: it cannot join the delegated
	// tree, so it gets a scope of its own instead, or its leaf if the
	// terminal won the race.
	let mut early = sleep();
	place(early.id());
	let scope = format!("app-poopterm-{}.scope", early.id());
	let leaf = format!("/tab-{}", early.id());
	let path = format!("/proc/{}/cgroup", early.id());
	let placed = (0..300).any(|_| {
		thread::sleep(Duration::from_millis(10));
		std::fs::read_to_string(&path)
			.is_ok_and(|text| text.trim_end().ends_with(&scope) || text.trim_end().ends_with(&leaf))
	});
	// The terminal, here the test process, sits in the `terminal` leaf of
	// its delegated scope.
	let terminal = format!(
		"app-poopterm-terminal-{}.scope/terminal",
		std::process::id()
	);
	let ours = moved(std::process::id(), &terminal);
	// Started now, inside the tree: it moves into a leaf of its own.
	let mut late = sleep();
	place(late.id());
	let late_leaf = moved(late.id(), &format!("/tab-{}", late.id()));
	for child in [&mut early, &mut late] {
		let _ = child.kill();
		let _ = child.wait();
	}
	assert!(placed, "{path}");
	assert!(ours, "not in {terminal}");
	assert!(late_leaf, "the late shell is not in its leaf");
	let own = std::fs::read_to_string("/proc/self/cgroup").unwrap();
	let dir = format!("/sys/fs/cgroup{}", own.trim_end().trim_start_matches("0::"));
	let scope_dir = Path::new(&dir).parent().unwrap();
	let control = std::fs::read_to_string(scope_dir.join("cgroup.subtree_control")).unwrap();
	if control.split_whitespace().any(|name| name == "cpu") {
		let weight = std::fs::read_to_string(format!("{dir}/cpu.weight")).unwrap();
		assert_eq!(weight.trim(), "400");
	}
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
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			None,
			None,
			|_| None,
		)
	});
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

#[test]
fn the_terminal_scope_is_delegated_to_us() {
	let message = terminal_scope(77);
	let body = &message.body;
	assert_eq!(body[0], Value::Str("app-poopterm-terminal-77.scope".into()));
	assert_eq!(message.flags, NO_REPLY_EXPECTED | NO_AUTO_START);
	let names: Vec<String> = properties(body).into_iter().map(|(name, _)| name).collect();
	assert_eq!(names, ["Description", "PIDs", "CollectMode", "Delegate"]);
	let pids = Value::Array("u".into(), vec![Value::U32(77)]);
	assert_eq!(property(body, "PIDs"), Some(pids));
	// We lay out its cgroup ourselves, so its weights are set inside it.
	assert_eq!(property(body, "Delegate"), Some(Value::Bool(true)));
	assert_eq!(property(body, "CPUWeight"), None);
	assert_eq!(property(body, "MemoryHigh"), None);
	assert_eq!(property(&start_scope(9, None).body, "Delegate"), None);
	let mut sent = message.clone();
	sent.serial = 1;
	assert_eq!(Message::decode(&sent.encode()), Ok(sent));
}

#[test]
fn the_worker_moves_the_terminal_before_any_shell() {
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
			None,
			Some(9),
			|_| None,
		)
	});
	pids.send(10).unwrap();
	let names: Vec<Value> = (0..2)
		.map(|_| next_unit(&mut systemd).body[0].clone())
		.collect();
	assert_eq!(
		names,
		[
			Value::Str("app-poopterm-terminal-9.scope".into()),
			Value::Str("app-poopterm-10.scope".into())
		]
	);
	drop(pids);
	worker.join().unwrap();
}

#[test]
fn the_terminal_moves_once_however_many_shells_follow() {
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
			None,
			Some(9),
			|_| None,
		)
	});
	for pid in 10..14 {
		pids.send(pid).unwrap();
	}
	drop(pids);
	worker.join().unwrap();
	let names: Vec<String> = (0..5)
		.map(|_| next_unit(&mut systemd).body[0].as_str().unwrap().to_owned())
		.collect();
	let terminal = names
		.iter()
		.filter(|name| name.contains("terminal"))
		.count();
	assert_eq!(terminal, 1, "{names:?}");
	assert_eq!(names[4], "app-poopterm-13.scope");
	// Nothing else was sent: the next read times out.
	systemd.set_timeout(Duration::from_millis(100));
	assert!(systemd.receive().is_err());
}

#[test]
fn a_lost_terminal_request_does_not_hold_back_the_shells() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			None,
			Some(9),
			|_| None,
		)
	});
	// The terminal's request goes out with nobody owning the name.
	pids.send(1).unwrap();
	thread::sleep(Duration::from_millis(200));
	let mut systemd = fake_systemd(&bus);
	pids.send(2).unwrap();
	let two = Value::Str("app-poopterm-2.scope".into());
	while next_unit(&mut systemd).body[0] != two {}
	drop(pids);
	worker.join().unwrap();
}

/// A delegation that never finds a cgroup: the mount does not exist.
fn nowhere() -> Delegation {
	Delegation {
		mount: std::env::temp_dir().join("poopterm-cgroup-no-mount"),
		attempts: 1,
	}
}

/// A fake delegated cgroup holding only `pid`, laid out as a tree.
fn fake_tree(name: &str, pid: u32) -> (std::path::PathBuf, Tree) {
	let dir = std::env::temp_dir().join(format!("poopterm-worker-{}-{name}", std::process::id()));
	let _ = std::fs::remove_dir_all(&dir);
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("cgroup.procs"), pid.to_string()).unwrap();
	let tree = Tree::adopt(&dir, pid, true, None).unwrap();
	(dir, tree)
}

#[test]
fn with_a_delegated_tree_shells_go_into_leaves_not_scopes() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut systemd = fake_systemd(&bus);
	let (dir, tree) = fake_tree("leaves", 9);
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let (asked, told) = mpsc::channel();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			None,
			Some(9),
			move |sent| {
				asked.send(sent).unwrap();
				Some(tree)
			},
		)
	});
	pids.send(10).unwrap();
	drop(pids);
	worker.join().unwrap();
	assert_eq!(told.recv(), Ok(true), "told that systemd was asked");
	let terminal = next_unit(&mut systemd);
	let name = Value::Str("app-poopterm-terminal-9.scope".into());
	assert_eq!(terminal.body[0], name);
	assert_eq!(
		std::fs::read_to_string(dir.join("tab-10/cgroup.procs")).unwrap(),
		"10"
	);
	// No scope of its own was asked for.
	systemd.set_timeout(Duration::from_millis(100));
	assert!(systemd.receive().is_err());
}

#[test]
fn a_shell_the_tree_cannot_take_gets_a_scope_of_its_own() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let mut systemd = fake_systemd(&bus);
	let (dir, tree) = fake_tree("fallback", 9);
	// The tree's cgroup is gone, so no leaf can be made in it.
	std::fs::remove_dir_all(&dir).unwrap();
	let (pids, receiver) = mpsc::channel();
	let address = bus.address.clone();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Connection::connect(&address, None),
			Some(4096),
			Some(9),
			move |_| Some(tree),
		)
	});
	pids.send(10).unwrap();
	drop(pids);
	worker.join().unwrap();
	next_unit(&mut systemd);
	let shell = next_unit(&mut systemd);
	assert_eq!(shell.body[0], Value::Str("app-poopterm-10.scope".into()));
	assert_eq!(property(&shell.body, "MemoryHigh"), Some(Value::U64(4096)));
}

#[test]
fn without_a_bus_a_delegated_tree_still_places_shells() {
	let (dir, tree) = fake_tree("no-bus", 9);
	let (pids, receiver) = mpsc::channel();
	let (asked, told) = mpsc::channel();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Err(io::ErrorKind::NotFound.into()),
			None,
			Some(9),
			move |sent| {
				asked.send(sent).unwrap();
				Some(tree)
			},
		)
	});
	pids.send(10).unwrap();
	pids.send(11).unwrap();
	drop(pids);
	worker.join().unwrap();
	assert_eq!(told.recv(), Ok(false), "nobody was asked for a scope");
	for pid in [10, 11] {
		let procs = std::fs::read_to_string(dir.join(format!("tab-{pid}/cgroup.procs")));
		assert_eq!(procs.unwrap(), pid.to_string());
	}
}

#[test]
fn without_a_terminal_there_is_no_tree_to_ask_for() {
	let (pids, receiver) = mpsc::channel::<u32>();
	let worker = thread::spawn(move || {
		run(
			receiver,
			true,
			|| Err(io::ErrorKind::NotFound.into()),
			None,
			None,
			|_| -> Option<Tree> { panic!("asked for a tree") },
		)
	});
	worker.join().unwrap();
	assert!(pids.send(1).is_err());
}

/// A fake cgroup2 mount holding `name` with `procs`, and the
/// `/proc/self/cgroup` and `/proc/meminfo` that put us in it.
fn fake_mount(
	name: &str,
	procs: &str,
	meminfo: &'static str,
) -> (std::path::PathBuf, impl Fn(&str) -> String + Send + 'static) {
	let mount = std::env::temp_dir().join(format!("poopterm-mount-{}-{name}", std::process::id()));
	let _ = std::fs::remove_dir_all(&mount);
	let dir = mount.join(name);
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("cgroup.procs"), procs).unwrap();
	std::fs::write(dir.join("cgroup.controllers"), "memory\n").unwrap();
	let cgroup = format!("0::/{name}\n");
	(mount, move |path: &str| match path {
		"/proc/self/cgroup" => cgroup.clone(),
		"/proc/meminfo" => meminfo.into(),
		other => panic!("read {other}"),
	})
}

/// Waits until the fake leaf `tab-<pid>` under `dir` lists the shell.
fn leaf_written(dir: &Path, pid: u32) -> bool {
	let procs = dir.join(format!("tab-{pid}/cgroup.procs"));
	(0..300).any(|_| {
		thread::sleep(Duration::from_millis(10));
		std::fs::read_to_string(&procs).is_ok_and(|text| text == pid.to_string())
	})
}

#[test]
fn without_a_bus_the_worker_lays_out_our_own_cgroup_with_the_memory_limit() {
	let (mount, read) = fake_mount("own", "77\n", "MemTotal: 8 kB\n");
	let pids = start(
		read,
		|_| true,
		|| Err(io::ErrorKind::NotFound.into()),
		77,
		Delegation {
			mount: mount.clone(),
			attempts: 1,
		},
	);
	pids.send(55).unwrap();
	let dir = mount.join("own");
	assert!(leaf_written(&dir, 55));
	assert_eq!(
		std::fs::read_to_string(dir.join("tab-55/memory.high")).unwrap(),
		"7168"
	);
	assert_eq!(
		std::fs::read_to_string(dir.join("terminal/cgroup.procs")).unwrap(),
		"77"
	);
}

#[test]
fn the_worker_lays_out_the_scope_systemd_made_even_with_shells_in_it() {
	let Some(bus) = TestBus::start() else {
		return;
	};
	let _systemd = fake_systemd(&bus);
	// Already moved, with a shell that was forked into the scope's root.
	let name = "app-poopterm-terminal-77.scope";
	let (mount, read) = fake_mount(name, "77\n12\n", "");
	let address = bus.address.clone();
	let pids = start(
		read,
		|_| true,
		move || Connection::connect(&address, None),
		77,
		Delegation {
			mount: mount.clone(),
			attempts: 1,
		},
	);
	pids.send(56).unwrap();
	let dir = mount.join(name);
	assert!(leaf_written(&dir, 56));
	assert_eq!(
		std::fs::read_to_string(dir.join("terminal/cgroup.procs")).unwrap(),
		"12"
	);
}
