//! Tests of user and host lookups against what other parts of the system
//! report for them.
use super::*;

/// What `uname -n` prints, the host name by another route, if it can run.
fn uname_n() -> Option<String> {
	let output = std::process::Command::new("uname")
		.arg("-n")
		.output()
		.ok()?;
	output
		.status
		.success()
		.then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
}

#[test]
fn the_host_name_is_what_uname_reports() {
	let name = hostname().expect("every system has a host name");
	if let Some(reported) = uname_n() {
		assert_eq!(name, reported);
	}
}

#[cfg(target_os = "linux")]
#[test]
fn the_host_name_is_the_kernels() {
	let kernel = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
	assert_eq!(hostname().unwrap(), kernel.trim_end_matches('\n'));
}

#[test]
fn the_host_name_has_no_terminator_and_fits_posix() {
	let name = hostname().unwrap();
	assert!(!name.contains('\0'), "{name:?}");
	assert!(name.len() <= 255, "{} bytes", name.len());
	assert!(!name.is_empty());
}

#[test]
fn files_we_create_are_owned_by_our_uid() {
	use std::os::unix::fs::MetadataExt;
	let file = std::env::temp_dir().join(format!("poopterm-sys-uid-{}", std::process::id()));
	std::fs::write(&file, b"x").unwrap();
	let owner = std::fs::metadata(&file).unwrap().uid();
	std::fs::remove_file(&file).unwrap();
	assert_eq!(owner, uid());
}

#[test]
fn root_has_an_absolute_login_shell() {
	let shell = shell(0).expect("every system lists root");
	assert!(shell.starts_with('/'), "{shell:?}");
	assert!(!shell.ends_with(['\n', ' ', '\0']), "{shell:?}");
}

#[test]
fn an_unknown_user_has_no_shell() {
	// Far above any user a system hands out, and not `nobody` (often 65534).
	for unknown in [3_999_999_999, u32::MAX - 1] {
		assert_eq!(shell(unknown), None, "{unknown}");
	}
}

/// The shell `/etc/passwd` lists for `uid`, where the file has a line for it.
fn passwd_shell(uid: u32) -> Option<String> {
	let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
	passwd.lines().find_map(|line| {
		let fields: Vec<&str> = line.split(':').collect();
		(fields.len() == 7 && fields[2].parse() == Ok(uid)).then(|| fields[6].to_owned())
	})
}

#[test]
fn the_shell_is_what_etc_passwd_lists() {
	// Root, and us: lookups by other users' IDs must not return ours.
	for uid in [0, uid()] {
		if let Some(listed) = passwd_shell(uid).filter(|listed| !listed.is_empty()) {
			assert_eq!(shell(uid), Some(listed), "uid {uid}");
		}
	}
}

#[test]
fn lookups_from_many_threads_agree() {
	// `getpwuid_r` keeps no state between calls, unlike `getpwuid`.
	let expected = (shell(0), shell(uid()));
	let found: Vec<_> = (0..8)
		.map(|_| std::thread::spawn(|| (shell(0), shell(uid()))))
		.collect();
	for thread in found {
		assert_eq!(thread.join().unwrap(), expected);
	}
}
