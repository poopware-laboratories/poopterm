//! The parent's side of spawning, without starting anything on a terminal:
//! building a command, finding its program, what is prepared before the
//! fork, the pipe that reports `exec` failures, and waiting.
use super::*;
use std::{ffi::CStr, fs::File, io::Write, os::fd::FromRawFd, os::unix::ffi::OsStringExt};

/// The strings a null-terminated pointer array prepared for `execve`
/// holds.
fn strings(pointers: &[*const libc::c_char]) -> Vec<Vec<u8>> {
	let (last, strings) = pointers.split_last().unwrap();
	assert!(last.is_null(), "the array ends with a null pointer");
	strings
		.iter()
		// SAFETY: each points into a `CString` the `Prepared` owns.
		.map(|&string| unsafe { CStr::from_ptr(string) }.to_bytes().to_vec())
		.collect()
}

/// A directory of our own under the temporary directory, emptied first.
fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-spawn-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir
}

/// Writes `contents` to `path` with permission bits `mode`.
fn file(path: &Path, contents: &[u8], mode: u32) {
	File::create(path).unwrap().write_all(contents).unwrap();
	fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn os(bytes: &[u8]) -> OsString {
	OsString::from_vec(bytes.to_vec())
}

#[test]
fn a_relative_program_is_made_absolute_from_our_directory() {
	let ours = std::env::current_dir().unwrap();
	let prepared = Prepared::new(&Command::new("./Cargo.toml")).unwrap();
	assert_eq!(
		prepared.path.as_bytes(),
		ours.join("Cargo.toml").as_os_str().as_bytes()
	);
}

#[test]
fn a_program_found_through_a_relative_path_entry_is_absolute() {
	let dir = scratch("relative-entry");
	file(&dir.join("poopterm-relative"), b"#!/bin/sh\n", 0o755);
	let ours = std::env::current_dir().unwrap();
	// The same directory, named relative to ours.
	let up: PathBuf = ours.components().skip(1).map(|_| "..").collect();
	let entry = up.join(dir.strip_prefix("/").unwrap());
	let command = Command::new("poopterm-relative").env("PATH", &entry);
	let prepared = Prepared::new(&command).unwrap();
	let path = Path::new(OsStr::from_bytes(prepared.path.as_bytes()));
	assert!(path.is_absolute(), "{path:?}");
	let expected = dir.join("poopterm-relative").canonicalize().unwrap();
	assert_eq!(path.canonicalize().unwrap(), expected);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_absolute_program_is_kept_as_written() {
	let prepared = Prepared::new(&Command::new("/bin/../bin/sh")).unwrap();
	assert_eq!(prepared.path.as_bytes(), b"/bin/../bin/sh");
}

#[test]
fn a_new_command_names_its_program_in_argv0() {
	let command = Command::new("zsh");
	assert_eq!(command.program, "zsh");
	assert_eq!(command.arg0, "zsh");
	assert!(command.args.is_empty());
	assert!(command.env.is_empty());
	assert_eq!(command.cwd, None);
}

#[test]
fn argv0_changes_the_name_but_not_the_program() {
	let command = Command::new("/bin/zsh").arg0("-zsh");
	assert_eq!(command.program, "/bin/zsh");
	assert_eq!(command.arg0, "-zsh");
}

#[test]
fn arguments_keep_their_order() {
	let command = Command::new("sh").arg("-c").arg("").arg("x");
	assert_eq!(command.args, ["-c", "", "x"]);
}

#[test]
fn the_last_directory_asked_wins() {
	let command = Command::new("sh").cwd("/a").cwd("/b");
	assert_eq!(command.cwd, Some(PathBuf::from("/b")));
}

#[test]
fn the_environment_starts_as_ours() {
	let ours: Vec<_> = std::env::vars_os().collect();
	assert_eq!(Command::new("sh").environment(), ours);
}

#[test]
fn a_new_variable_is_added_once_and_the_rest_kept() {
	let ours = std::env::vars_os().count();
	let vars = Command::new("sh")
		.env("POOPTERM_SETUP_NEW", "1")
		.environment();
	assert_eq!(vars.len(), ours + 1);
	assert_eq!(
		vars.last().unwrap(),
		&("POOPTERM_SETUP_NEW".into(), "1".into())
	);
}

#[test]
fn a_variable_of_ours_is_replaced_not_repeated() {
	let (name, _) = std::env::vars_os()
		.next()
		.expect("tests run with an environment");
	let vars = Command::new("sh").env(&name, "replaced").environment();
	let values: Vec<_> = vars.iter().filter(|(other, _)| *other == name).collect();
	assert_eq!(values, [&(name.clone(), "replaced".into())]);
	assert_eq!(vars.len(), std::env::vars_os().count());
}

#[test]
fn names_differing_only_in_case_are_different_variables() {
	let vars = Command::new("sh")
		.env("POOPTERM_CASE", "upper")
		.env("poopterm_case", "lower")
		.environment();
	assert!(vars.contains(&("POOPTERM_CASE".into(), "upper".into())));
	assert!(vars.contains(&("poopterm_case".into(), "lower".into())));
}

#[test]
fn argv_is_the_name_then_the_arguments_then_null() {
	let command = Command::new("/bin/sh").arg0("-sh").arg("-c").arg("true");
	let prepared = Prepared::new(&command).unwrap();
	assert_eq!(strings(&prepared.argv), [&b"-sh"[..], b"-c", b"true"]);
	assert_eq!(prepared.path.as_bytes(), b"/bin/sh");
}

#[test]
fn bytes_that_are_not_utf8_reach_argv_unchanged() {
	let command = Command::new("/bin/sh").arg(os(b"\xff\xfe"));
	let prepared = Prepared::new(&command).unwrap();
	assert_eq!(strings(&prepared.argv)[1], b"\xff\xfe");
}

#[test]
fn envp_holds_name_equals_value_for_each_variable() {
	let command = Command::new("/bin/sh")
		.env("POOPTERM_EQUALS", "a=b=c")
		.env("POOPTERM_EMPTY", "");
	let prepared = Prepared::new(&command).unwrap();
	let envp = strings(&prepared.envp);
	assert!(envp.contains(&b"POOPTERM_EQUALS=a=b=c".to_vec()));
	assert!(envp.contains(&b"POOPTERM_EMPTY=".to_vec()));
	assert_eq!(envp.len(), command.environment().len());
}

#[test]
fn the_program_is_looked_for_in_the_commands_own_path() {
	let dir = scratch("own-path");
	file(&dir.join("poopterm-own"), b"#!/bin/sh\n", 0o755);
	let command = Command::new("poopterm-own").env("PATH", &dir);
	let prepared = Prepared::new(&command).unwrap();
	assert_eq!(
		prepared.path.as_bytes(),
		dir.join("poopterm-own").as_os_str().as_bytes()
	);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_program_missing_from_the_commands_path_is_not_found() {
	let dir = scratch("empty-path");
	let error = Prepared::new(&Command::new("sh").env("PATH", &dir))
		.err()
		.unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_in_path_that_is_not_executable_is_passed_over() {
	let first = scratch("plain-first");
	let second = scratch("plain-second");
	file(&first.join("poopterm-twice"), b"#!/bin/sh\n", 0o644);
	file(&second.join("poopterm-twice"), b"#!/bin/sh\n", 0o700);
	let search = std::env::join_paths([&first, &second]).unwrap();
	let prepared = Prepared::new(&Command::new("poopterm-twice").env("PATH", search)).unwrap();
	assert_eq!(
		prepared.path.as_bytes(),
		second.join("poopterm-twice").as_os_str().as_bytes()
	);
	fs::remove_dir_all(first).unwrap();
	fs::remove_dir_all(second).unwrap();
}

#[test]
fn a_nul_byte_anywhere_is_invalid_input() {
	let commands = [
		Command::new("/bin/sh").arg("a\0b"),
		Command::new("/bin/sh").arg0("s\0h"),
		Command::new("/bin/sh").env("POOPTERM_NUL", "a\0b"),
		Command::new("/bin/sh").env("POOPTERM\0NUL", "a"),
		Command::new("/bin/sh").cwd("/tmp\0x"),
		Command::new("/bin/s\0h"),
	];
	for command in commands {
		let error = Prepared::new(&command).err().unwrap();
		assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
	}
}

#[test]
fn a_nul_byte_in_a_bare_name_finds_nothing() {
	let error = Prepared::new(&Command::new("s\0h")).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
}

#[test]
fn the_directory_is_prepared_only_when_asked() {
	assert_eq!(Prepared::new(&Command::new("/bin/sh")).unwrap().cwd, None);
	let prepared = Prepared::new(&Command::new("/bin/sh").cwd("/var")).unwrap();
	assert_eq!(prepared.cwd.unwrap().as_bytes(), b"/var");
}

#[test]
fn the_prepared_signal_set_is_empty() {
	let prepared = Prepared::new(&Command::new("/bin/sh")).unwrap();
	for signal in 1..=LAST_SIGNAL.min(31) {
		// SAFETY: reads the set the `Prepared` owns.
		let member = unsafe { libc::sigismember(&prepared.no_signals, signal) };
		assert_eq!(member, 0, "{signal}");
	}
}

#[test]
fn the_prepared_action_is_the_default_one() {
	let prepared = Prepared::new(&Command::new("/bin/sh")).unwrap();
	assert_eq!(prepared.default_action.sa_sigaction, libc::SIG_DFL);
	assert_eq!(prepared.default_action.sa_flags, 0);
}

#[test]
fn descriptors_are_marked_up_to_the_limit_within_bounds() {
	assert_eq!(descriptor_limit(1024), 1024);
	assert_eq!(descriptor_limit(MOST_DESCRIPTORS.into()), MOST_DESCRIPTORS);
	assert_eq!(
		descriptor_limit(libc::c_long::from(MOST_DESCRIPTORS) + 1),
		MOST_DESCRIPTORS
	);
	assert_eq!(descriptor_limit(libc::c_long::MAX), MOST_DESCRIPTORS);
	assert_eq!(descriptor_limit(0), 3);
	assert_eq!(descriptor_limit(4), 4);
}

#[test]
fn no_limit_on_descriptors_marks_the_most() {
	assert_eq!(descriptor_limit(-1), MOST_DESCRIPTORS);
}

#[test]
fn an_executable_file_is_executable() {
	let dir = scratch("executable");
	for (name, mode, expected) in [
		("owner", 0o700, true),
		("group", 0o610, true),
		("others", 0o601, true),
		("plain", 0o644, false),
	] {
		file(&dir.join(name), b"", mode);
		assert_eq!(is_executable(&dir.join(name)), expected, "{name}");
	}
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn directories_and_missing_files_are_not_executable() {
	let dir = scratch("not-executable");
	assert!(
		!is_executable(&dir),
		"a directory has x bits but is no program"
	);
	assert!(!is_executable(&dir.join("missing")));
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_link_is_executable_when_its_target_is() {
	let dir = scratch("links");
	file(&dir.join("target"), b"", 0o755);
	std::os::unix::fs::symlink(dir.join("target"), dir.join("good")).unwrap();
	std::os::unix::fs::symlink(dir.join("gone"), dir.join("dangling")).unwrap();
	assert!(is_executable(&dir.join("good")));
	assert!(!is_executable(&dir.join("dangling")));
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn relative_path_entries_stay_relative() {
	let found = resolve(OsStr::new("zsh"), Some(OsStr::new("bin")), |_| true);
	assert_eq!(found, Some(PathBuf::from("bin/zsh")));
}

#[test]
fn a_leading_or_trailing_empty_entry_is_the_current_directory() {
	let in_dot = |path: &Path| path.parent() == Some(Path::new("."));
	for path in [":/a", "/a:"] {
		let found = resolve(OsStr::new("zsh"), Some(OsStr::new(path)), in_dot);
		assert_eq!(found, Some(PathBuf::from("./zsh")), "{path}");
	}
}

#[test]
fn an_empty_path_is_the_current_directory() {
	let found = resolve(OsStr::new("zsh"), Some(OsStr::new("")), |_| true);
	assert_eq!(found, Some(PathBuf::from("./zsh")));
}

#[test]
fn path_entries_that_are_not_utf8_are_searched() {
	let path = os(b"/\xff");
	let found = resolve(OsStr::new("zsh"), Some(&path), |_| true);
	assert_eq!(found.unwrap().into_os_string(), os(b"/\xff/zsh"));
}

#[test]
fn a_slash_anywhere_skips_the_search() {
	for program in ["bin/", "/zsh", "a/b"] {
		let found = resolve(OsStr::new(program), None, |_| false);
		assert_eq!(found, Some(PathBuf::from(program)), "{program}");
	}
}

#[test]
fn every_entry_is_tried_in_order_until_one_holds_it() {
	let tried = std::cell::RefCell::new(Vec::new());
	let found = resolve(OsStr::new("zsh"), Some(OsStr::new("/a:/b:/c")), |path| {
		tried.borrow_mut().push(path.to_path_buf());
		path.starts_with("/b")
	});
	assert_eq!(found, Some(PathBuf::from("/b/zsh")));
	assert_eq!(
		tried.into_inner(),
		[Path::new("/a/zsh"), Path::new("/b/zsh")]
	);
}

fn close_on_exec(fd: &impl AsRawFd) -> bool {
	// SAFETY: reads the flags of a descriptor we own.
	let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
	assert!(flags >= 0);
	flags & libc::FD_CLOEXEC != 0
}

/// A copy of a descriptor of ours that does not close on exec.
fn inheritable() -> OwnedFd {
	let file = File::open("Cargo.toml").unwrap();
	// SAFETY: duplicates a descriptor we own and checks the result.
	let fd = check(unsafe { libc::dup(file.as_raw_fd()) }).unwrap();
	// SAFETY: `fd` was just made and nothing else owns it.
	unsafe { OwnedFd::from_raw_fd(fd) }
}

#[test]
fn descriptors_in_the_range_are_marked_close_on_exec() {
	let fd = inheritable();
	assert!(!close_on_exec(&fd), "dup leaves the flag off");
	mark_close_on_exec(fd.as_raw_fd()..fd.as_raw_fd() + 1);
	assert!(close_on_exec(&fd));
}

#[test]
fn descriptors_outside_the_range_are_left_alone() {
	let (marked, kept) = (inheritable(), inheritable());
	mark_close_on_exec(marked.as_raw_fd()..marked.as_raw_fd() + 1);
	assert!(close_on_exec(&marked));
	assert!(
		!close_on_exec(&kept),
		"the range ends before {}",
		kept.as_raw_fd()
	);
}

#[test]
fn the_range_is_marked_whole_with_its_gaps_skipped() {
	let fds = [inheritable(), inheritable(), inheritable()];
	let low = fds.iter().map(AsRawFd::as_raw_fd).min().unwrap();
	let high = fds.iter().map(AsRawFd::as_raw_fd).max().unwrap();
	// Numbers between them may be closed, or another test's; neither stops it.
	mark_close_on_exec(low..high + 1);
	assert!(fds.iter().all(close_on_exec));
}

#[test]
fn an_empty_or_negative_range_marks_nothing() {
	let fd = inheritable();
	mark_close_on_exec(fd.as_raw_fd()..fd.as_raw_fd());
	mark_close_on_exec(-3..0);
	assert!(!close_on_exec(&fd));
}

#[test]
fn both_ends_of_the_report_pipe_close_on_exec() {
	let (read, write) = report_pipe().unwrap();
	assert!(close_on_exec(&read));
	assert!(close_on_exec(&write));
}

#[test]
fn a_report_pipe_closed_unwritten_means_exec_worked() {
	let (read, write) = report_pipe().unwrap();
	drop(write);
	read_report(&read).unwrap();
}

#[test]
fn a_report_carries_the_childs_errno() {
	let (read, write) = report_pipe().unwrap();
	(&write).write_all(&libc::EACCES.to_ne_bytes()).unwrap();
	let error = read_report(&read).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::EACCES));
}

#[test]
fn a_short_report_is_invalid_data() {
	let (read, write) = report_pipe().unwrap();
	(&write).write_all(&[1, 2]).unwrap();
	let error = read_report(&read).err().unwrap();
	assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_report_that_cannot_be_read_is_an_error() {
	// Not a pipe's write end: FreeBSD's pipes read both ways, and block.
	let write_only = fs::OpenOptions::new()
		.write(true)
		.open("/dev/null")
		.unwrap();
	let error = read_report(&io::PipeReader::from(OwnedFd::from(write_only)))
		.err()
		.unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::EBADF));
}

#[test]
fn waiting_for_a_process_that_is_not_ours_fails() {
	// The init process is never our child.
	assert_eq!(
		wait_for(1).err().unwrap().raw_os_error(),
		Some(libc::ECHILD)
	);
}

#[test]
fn c_strings_refuse_only_nul() {
	assert_eq!(c_string(b"").unwrap().as_bytes(), b"");
	assert_eq!(c_string(b"\xff=").unwrap().as_bytes(), b"\xff=");
	assert_eq!(
		c_string(b"a\0").err().unwrap().raw_os_error(),
		Some(libc::EINVAL)
	);
}
