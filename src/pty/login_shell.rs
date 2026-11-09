//! Which shell a new tab runs: `$SHELL` when it names one that exists, else
//! the login shell from the account database (what `chsh` sets), else
//! `/bin/sh`. A program started from the Dock or Finder has no `$SHELL`.

use std::ffi::CStr;

/// The shell to run. `usable` says whether a path can be started.
pub(super) fn choose(
	env: Option<&str>,
	account: Option<&str>,
	usable: impl Fn(&str) -> bool,
) -> String {
	// Only absolute paths: a bare name would be found in the current directory.
	let good = |path: &&str| path.starts_with('/') && usable(path);
	[env, account]
		.into_iter()
		.flatten()
		.find(good)
		.unwrap_or("/bin/sh")
		.to_string()
}

/// The login shell the account database lists for the current user.
pub(super) fn account_shell() -> Option<String> {
	let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
	let mut buffer = vec![0u8; 4096];
	let mut found: *mut libc::passwd = std::ptr::null_mut();
	// SAFETY: `entry`, `buffer` and `found` outlive the call and the buffer's
	// length is passed with it; `getpwuid_r` writes only inside them.
	let status = unsafe {
		libc::getpwuid_r(
			libc::getuid(),
			entry.as_mut_ptr(),
			buffer.as_mut_ptr().cast(),
			buffer.len(),
			&mut found,
		)
	};
	if status != 0 || found.is_null() {
		return None;
	}
	// SAFETY: a zero status with a non-null `found` means `getpwuid_r` filled
	// in `entry`, which we own; we read it, not the pointer it handed back.
	let shell = unsafe { entry.assume_init_ref() }.pw_shell;
	if shell.is_null() {
		return None;
	}
	// SAFETY: `pw_shell` is a NUL-terminated string inside `buffer`, which is
	// alive here; it is copied before `buffer` is dropped.
	let shell = unsafe { CStr::from_ptr(shell) };
	shell.to_str().ok().map(str::to_string)
}

/// The shell for a new tab.
pub(crate) fn login_shell() -> String {
	let env = std::env::var("SHELL").ok();
	let account = account_shell();
	choose(env.as_deref(), account.as_deref(), is_program)
}

/// Whether `path` is an existing file (a directory cannot be run as a shell).
pub(super) fn is_program(path: &str) -> bool {
	std::path::Path::new(path).is_file()
}

#[cfg(test)]
#[path = "login_shell_tests.rs"]
mod tests;
