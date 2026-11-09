//! Which shell a new tab runs: `$SHELL`, else the account's login shell.
use super::*;

fn exists(path: &str) -> bool {
	matches!(path, "/bin/sh" | "/bin/zsh" | "/opt/homebrew/bin/fish")
}

#[test]
fn the_environment_wins_when_it_names_a_usable_shell() {
	let chosen = choose(Some("/bin/zsh"), Some("/opt/homebrew/bin/fish"), exists);
	assert_eq!(chosen, "/bin/zsh");
}

#[test]
fn a_dock_launch_without_shell_uses_the_login_shell() {
	let chosen = choose(None, Some("/opt/homebrew/bin/fish"), exists);
	assert_eq!(chosen, "/opt/homebrew/bin/fish");
}

#[test]
fn an_empty_environment_value_counts_as_unset() {
	let chosen = choose(Some(""), Some("/opt/homebrew/bin/fish"), exists);
	assert_eq!(chosen, "/opt/homebrew/bin/fish");
}

#[test]
fn a_shell_that_is_gone_falls_back_to_the_login_shell() {
	let chosen = choose(
		Some("/usr/local/bin/gone"),
		Some("/opt/homebrew/bin/fish"),
		exists,
	);
	assert_eq!(chosen, "/opt/homebrew/bin/fish");
}

#[test]
fn a_relative_name_is_not_trusted() {
	// Looking `fish` up in the current directory would run whatever is there.
	let chosen = choose(Some("fish"), Some("/bin/zsh"), exists);
	assert_eq!(chosen, "/bin/zsh");
}

#[test]
fn with_nothing_usable_the_posix_shell_runs() {
	assert_eq!(choose(None, None, exists), "/bin/sh");
	assert_eq!(choose(Some("nope"), Some("/gone"), exists), "/bin/sh");
}

#[test]
fn the_account_database_names_a_shell_for_this_user() {
	// Some CI containers run as a user with no account entry: then there is none.
	if let Some(shell) = account_shell() {
		assert!(shell.starts_with('/'), "{shell}");
		assert!(!shell.contains('\0'));
	}
}

#[test]
fn the_real_lookup_never_returns_an_empty_name() {
	assert!(login_shell().starts_with('/'));
}
