//! Which shell a new tab runs: `$SHELL` when it names one that exists, else
//! the login shell from the account database (what `chsh` sets), else
//! `/bin/sh`. A program started from the Dock or Finder has no `$SHELL`.

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
	crate::sys::user::shell(crate::sys::user::uid())
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
