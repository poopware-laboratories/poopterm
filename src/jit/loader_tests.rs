//! Our loader on libraries the system compiler builds: what dyld would do
//! for them, done by us, and run.
use std::fs;

use super::*;
use crate::jit::macho::tests::built;

/// The test library with its entry named as a command's.
const AS_COMMAND: &[&str] = &["-Dpoopterm_test_entry=poopterm_entry"];

fn call(entry: *mut c_void, value: i32) -> i32 {
	// SAFETY: the test library's entry takes an int and returns one.
	let function: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(entry) };
	function(value)
}

#[cfg(target_arch = "aarch64")]
#[test]
fn a_loaded_library_runs_with_its_pointers_imports_and_constructor() {
	let Some(path) = built(AS_COMMAND) else {
		return;
	};
	let mut arena = Arena::plain(64 << 20);
	let loaded = load(&path, &mut arena).unwrap();
	let entry = loaded.fresh_entry();
	assert_eq!(call(entry, 1), 5, "strlen through a rebased pointer");
	assert_eq!(call(entry, 2), 42, "the constructor ran");
	assert_eq!(call(entry, 3), 1, "a missing weak import is null");
	assert_eq!(call(entry, 5), 1234, "snprintf and atoi bound");
	assert!((loaded.address()..loaded.address() + (1 << 20)).contains(&(entry as usize)));
}

#[cfg(target_arch = "aarch64")]
#[test]
fn each_fresh_entry_starts_with_the_globals_as_loaded() {
	let Some(path) = built(AS_COMMAND) else {
		return;
	};
	let mut arena = Arena::plain(64 << 20);
	let loaded = load(&path, &mut arena).unwrap();
	let entry = loaded.fresh_entry();
	assert_eq!(call(entry, 4), 1);
	assert_eq!(call(entry, 4), 2, "a global counts within a run");
	let entry = loaded.fresh_entry();
	assert_eq!(call(entry, 4), 1, "and starts over for the next");
	assert_eq!(call(entry, 2), 42, "with the constructor run again");
	assert!(format!("{loaded:?}").starts_with("Loaded"));
}

#[cfg(target_arch = "aarch64")]
#[test]
fn the_loader_agrees_with_dyld() {
	let Some(path) = built(AS_COMMAND) else {
		return;
	};
	let name = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
	let dyld = dl::Library::open(&name).unwrap();
	let theirs = dyld.symbol(c"poopterm_entry").unwrap().as_ptr();
	let mut arena = Arena::plain(64 << 20);
	let ours = load(&path, &mut arena).unwrap();
	let ours = ours.fresh_entry();
	for value in [1, 2, 3, 5, 0] {
		assert_eq!(call(ours, value), call(theirs, value), "{value}");
	}
}

#[test]
fn a_library_without_an_entry_a_missing_file_and_a_text_are_refused() {
	let Some(plain) = built(&[]) else {
		return;
	};
	let mut arena = Arena::plain(64 << 20);
	let error = load(&plain, &mut arena).unwrap_err();
	assert!(error.contains("poopterm_entry"), "{error}");
	assert!(load(&plain.with_file_name("missing.dylib"), &mut arena).is_err());
	let error = load(&plain.with_file_name("lib.c"), &mut arena).unwrap_err();
	assert!(error.contains("Mach-O"), "{error}");
}

#[test]
fn an_unresolvable_strong_import_names_the_symbol() {
	let Some(path) = built(AS_COMMAND) else {
		return;
	};
	let bytes = fs::read(&path).unwrap();
	let library = macho::parse(&bytes).unwrap();
	let mut arena = Arena::plain(64 << 20);
	let mut nothing = |import: &macho::Import| {
		(import.name == "_strlen")
			.then_some(0usize)
			.filter(|_| false)
	};
	let error = place(&bytes, &library, &mut arena, &mut nothing).unwrap_err();
	assert!(error.contains("which this system lacks"), "{error}");
	let mut full = |_: &macho::Import| None;
	let mut tiny = Arena::plain(16384);
	assert!(
		place(&bytes, &library, &mut tiny, &mut full)
			.unwrap_err()
			.contains("no room")
	);
}
