//! When our loader may run code, StikDebug's link, and the `jit` command
//! where JIT is not needed (here).
use super::*;

#[test]
fn macos_and_the_simulator_always_run_unsigned_code() {
	for major in [17, 26, 27] {
		for debugged in [false, true] {
			assert_eq!(strategy(false, major, debugged, false), Strategy::Plain);
		}
	}
}

#[test]
fn an_ios_device_needs_a_debugger() {
	assert_eq!(
		strategy(true, 17, false, false),
		Strategy::Unavailable(NEEDS_JIT)
	);
	assert_eq!(
		strategy(true, 27, false, true),
		Strategy::Unavailable(NEEDS_JIT)
	);
	assert_eq!(
		strategy(true, 18, true, false),
		Strategy::Plain,
		"before TXM, attaching is enough"
	);
}

#[test]
fn ios_26_and_later_also_need_a_prepared_region() {
	assert_eq!(
		strategy(true, 26, true, false),
		Strategy::Unavailable(NEEDS_SCRIPT)
	);
	assert_eq!(strategy(true, 27, true, true), Strategy::Prepared);
	assert_eq!(
		strategy(true, 25, true, true),
		Strategy::Plain,
		"the region is not needed"
	);
}

#[test]
fn the_stikdebug_link_names_the_app_and_process_and_the_script_on_txm() {
	assert_eq!(
		stikdebug_url("labs.poopterm", 42, false),
		"stikdebug://enable-jit?bundle-id=labs.poopterm&pid=42"
	);
	assert_eq!(
		stikdebug_url("labs.poopterm", 7, true),
		"stikdebug://enable-jit?bundle-id=labs.poopterm&pid=7&script-name=universal.js"
	);
}

#[test]
fn the_jit_command_reports_and_checks_its_arguments() {
	let never = || false;
	let (text, status) = command(&[], &never);
	assert_eq!(status, 0);
	assert!(text.starts_with("JIT: "), "{text}");
	assert_eq!(command(&["status".into()], &never).1, 0);
	let (text, status) = command(&["enable".into(), "8".into()], &never);
	assert_eq!(
		(status, text.as_str()),
		(2, "jit: the region is 16 to 1024 MB\n")
	);
	assert_eq!(command(&["enable".into(), "lots".into()], &never).1, 2);
	let (text, status) = command(&["bogus".into()], &never);
	assert_eq!(status, 2);
	assert!(text.starts_with("usage: jit"));
}

#[cfg(not(target_os = "ios"))]
#[test]
fn here_jit_is_not_needed() {
	let (text, status) = command(&["enable".into()], &|| false);
	assert_eq!(
		(status, text.as_str()),
		(0, "jit: not needed here; this system runs unsigned code\n")
	);
	assert!(command(&[], &|| false).0.contains("not needed here"));
	assert!(major_version() >= 11);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn open_uses_dyld_first_and_our_loader_when_forced() {
	let Some(path) = crate::jit::macho::tests::built(&["-Dpoopterm_test_entry=poopterm_entry"])
	else {
		return;
	};
	assert!(matches!(open(&path, false), Ok(Opened::Dyld(_))));
	let Ok(Opened::Ours(first)) = open(&path, true) else {
		panic!("our loader");
	};
	let Ok(Opened::Ours(again)) = open(&path, true) else {
		panic!("our loader");
	};
	assert!(Arc::ptr_eq(&first, &again), "loaded once, reused");
	assert!(Opened::Ours(first).entry().is_some());
	assert!(
		command(&[], &|| false).0.contains("code loaded: "),
		"the status counts it"
	);
	let missing = path.with_file_name("nothing.dylib");
	assert!(open(&missing, false).is_err());
}
