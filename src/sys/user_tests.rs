//! Tests of user and host lookups against what other parts of the system
//! report for them.
use super::*;
use std::os::unix::fs::MetadataExt;

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
	let dir = std::env::temp_dir().join(format!("poopterm-uid-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let file = dir.join("owned");
	std::fs::write(&file, b"x").unwrap();
	let (dir_owner, file_owner) = (
		std::fs::metadata(&dir).unwrap().uid(),
		std::fs::metadata(&file).unwrap().uid(),
	);
	std::fs::remove_dir_all(&dir).unwrap();
	assert_eq!(file_owner, uid());
	assert_eq!(dir_owner, uid());
}

#[test]
fn the_uid_is_what_id_reports() {
	let Ok(output) = std::process::Command::new("id").arg("-ru").output() else {
		return; // No `id` to ask.
	};
	assert!(output.status.success());
	let reported: u32 = String::from_utf8(output.stdout)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	assert_eq!(reported, uid());
}

#[cfg(target_os = "linux")]
#[test]
fn the_uid_is_the_real_one_the_kernel_lists_for_this_process() {
	let status = std::fs::read_to_string("/proc/self/status").unwrap();
	let ids = status
		.lines()
		.find_map(|line| line.strip_prefix("Uid:"))
		.unwrap();
	// Real, effective, saved and file system user IDs; ours is the real one.
	let real: u32 = ids.split_whitespace().next().unwrap().parse().unwrap();
	assert_eq!(real, uid());
}

#[test]
fn the_uid_does_not_change_between_calls_or_threads() {
	let here = uid();
	let there = std::thread::spawn(uid).join().unwrap();
	assert_eq!((here, there), (uid(), uid()));
}

#[test]
fn root_has_an_absolute_login_shell() {
	let shell = shell(0).expect("every system lists root");
	assert!(shell.starts_with('/'), "{shell:?}");
	assert!(!shell.ends_with(['\n', ' ', '\0']), "{shell:?}");
}

#[test]
fn an_unknown_user_has_no_shell() {
	// Far above any user a system hands out, and not `nobody` (65534, or -2
	// on macOS).
	for unknown in [2_000_000_123, 3_999_999_999] {
		assert_eq!(shell(unknown), None, "{unknown}");
	}
}

// macOS keeps accounts in its directory service, not `/etc/passwd`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
/// The shell `/etc/passwd` lists for `uid`, where the file has a line for it.
fn passwd_shell(uid: u32) -> Option<String> {
	let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
	passwd.lines().find_map(|line| {
		let fields: Vec<&str> = line.split(':').collect();
		(fields.len() == 7 && fields[2].parse() == Ok(uid)).then(|| fields[6].to_owned())
	})
}

// macOS keeps accounts in its directory service, not `/etc/passwd`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
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

// macOS keeps accounts in its directory service, not `/etc/passwd`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn every_account_in_etc_passwd_has_the_shell_its_first_line_lists() {
	// Several lines may share an ID (FreeBSD's `toor` is root again); the
	// lookup returns the first.
	let Ok(passwd) = std::fs::read_to_string("/etc/passwd") else {
		return;
	};
	let ids = passwd
		.lines()
		.filter_map(|line| line.split(':').nth(2)?.parse().ok());
	for uid in ids.collect::<std::collections::BTreeSet<u32>>() {
		let listed = passwd_shell(uid).unwrap();
		assert_eq!(shell(uid), Some(listed), "uid {uid}");
	}
}

#[test]
fn the_terminal_keeps_the_host_name_the_system_reports() {
	assert_eq!(crate::term::iterm::hostname(), hostname().unwrap());
}
