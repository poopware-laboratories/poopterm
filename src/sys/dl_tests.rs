//! Shared libraries opened at run time: symbols, handles and errors.
use super::*;

/// The C library, which every process here has loaded.
#[cfg(target_os = "linux")]
const LIBC: &CStr = c"libc.so.6";
#[cfg(target_os = "freebsd")]
const LIBC: &CStr = c"libc.so.7";

#[test]
fn the_c_library_has_its_functions_and_not_made_up_ones() {
	let libc = Library::open(LIBC).unwrap();
	assert!(libc.symbol(c"getpid").is_some());
	assert!(libc.symbol(c"poopterm_no_such_function").is_none());
}

#[test]
fn a_symbol_is_found_at_the_same_address_each_time() {
	let libc = Library::open(LIBC).unwrap();
	assert_eq!(libc.symbol(c"getpid"), libc.symbol(c"getpid"));
	assert_ne!(libc.symbol(c"getpid"), libc.symbol(c"getppid"));
}

#[test]
fn closing_one_handle_leaves_another_to_the_same_library_open() {
	let first = Library::open(LIBC).unwrap();
	let second = Library::open(LIBC).unwrap();
	let address = first.symbol(c"getpid");
	drop(first);
	assert_eq!(second.symbol(c"getpid"), address);
}

#[test]
fn each_failure_reports_its_own_library_not_an_earlier_one() {
	let Err(first) = Library::open(c"libpoopterm-first-missing.so.1") else {
		panic!("opened a library that does not exist");
	};
	let Err(second) = Library::open(c"libpoopterm-second-missing.so.1") else {
		panic!("opened a library that does not exist");
	};
	assert!(first.contains("first-missing"), "{first}");
	assert!(second.contains("second-missing"), "{second}");
	assert!(!second.contains("first-missing"), "{second}");
}

#[test]
fn a_file_that_is_not_a_library_is_an_error_naming_it() {
	let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
	let name = std::ffi::CString::new(path).unwrap();
	let Err(message) = Library::open(&name) else {
		panic!("opened Cargo.toml as a library");
	};
	assert!(message.contains("Cargo.toml"), "{message}");
}

#[test]
fn an_error_already_read_is_not_reported_again() {
	// `dlerror` hands each message out once; the next read finds none.
	assert!(Library::open(c"libpoopterm-read-once.so.1").is_err());
	assert_eq!(last_error(), "unknown error");
}
