//! When our loader may run code, StikDebug's link, and the `jit` command
//! where JIT is not needed (here).
use super::*;
use crate::sys::jit::Arena;

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

#[test]
fn the_messages_say_to_launch_from_stikdebug_and_choose_its_script() {
	assert!(NEEDS_JIT.contains("launch Poopterm from StikDebug"));
	assert!(NEEDS_SCRIPT.contains("universal script"));
	assert!(NEEDS_SCRIPT.contains("jit enable"));
}

#[test]
fn the_loader_switch_sets_its_flag_and_checks_its_argument() {
	use std::sync::atomic::{AtomicBool, Ordering};
	let flag = AtomicBool::new(false);
	let (text, status) = loader_switch(Some("on"), &flag);
	assert_eq!(status, 0);
	assert!(text.contains("own loader"), "{text}");
	assert!(flag.load(Ordering::Relaxed));
	let (text, status) = loader_switch(Some("off"), &flag);
	assert_eq!(status, 0);
	assert!(text.contains("dyld first"), "{text}");
	assert!(!flag.load(Ordering::Relaxed));
	for bad in [None, Some("maybe"), Some("")] {
		assert_eq!(loader_switch(bad, &flag).1, 2, "{bad:?}");
	}
	assert!(
		!flag.load(Ordering::Relaxed),
		"a bad argument changes nothing"
	);
}

#[test]
fn the_jit_command_routes_loader_and_rejects_a_missing_choice() {
	assert_eq!(command(&["loader".into()], &|| false).1, 2);
	assert!(
		command(&["bogus".into()], &|| false)
			.0
			.contains("jit loader on|off")
	);
}

#[test]
fn enabling_says_what_it_asks_before_it_asks_where_a_device_would() {
	let said = std::cell::RefCell::new(String::new());
	let (_, status) = command_with(&["enable".into()], &|| false, &|text| {
		said.borrow_mut().push_str(text)
	});
	// Off a device nothing is asked, so nothing is said either.
	assert_eq!((status, said.borrow().as_str()), (0, ""));
}

/// A region standing in for the one a debugger prepares, made executable
/// here as the kernel lets a test do, installed as the app's arena:
/// its start and length. `remove_test_region` takes it back.
pub(crate) fn install_test_region(len: usize) -> (usize, usize) {
	let region = crate::sys::jit::test_executable_region(len);
	// SAFETY: a region made executable for this test alone and never freed.
	let arena = unsafe { Arena::prepared(region, len) }.unwrap();
	install_arena(Some(arena));
	(region, len)
}

pub(crate) fn remove_test_region() {
	install_arena(None);
}

#[test]
fn a_dual_region_comes_from_a_prepared_arena_with_a_view_and_is_remembered() {
	assert!(
		dual_region(16384).is_none(),
		"none without a prepared arena"
	);
	let (start, len) = install_test_region(4 * 16384);
	let (run, view) = dual_region(2 * 16384 - 5).unwrap();
	assert_eq!(run, start, "the first piece");
	assert_ne!(view, run, "a view apart from where it runs");
	assert!(owns_region(run) && !owns_region(run + 1) && !owns_region(view));
	let (second, _) = dual_region(16384).unwrap();
	assert_eq!(second, start + 2 * 16384, "whole pages, one after another");
	assert!(dual_region(len).is_none(), "no room left");
	remove_test_region();
	assert!(dual_region(1).is_none());
}
