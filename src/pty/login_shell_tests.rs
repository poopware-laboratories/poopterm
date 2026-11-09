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

#[test]
fn the_login_shell_is_only_asked_about_when_the_environment_fails() {
	let asked = std::cell::RefCell::new(Vec::new());
	let probe = |path: &str| {
		asked.borrow_mut().push(path.to_string());
		true
	};
	assert_eq!(choose(Some("/bin/zsh"), Some("/bin/sh"), probe), "/bin/zsh");
	assert_eq!(*asked.borrow(), ["/bin/zsh"]);
}

#[test]
fn a_relative_name_is_never_probed_on_the_disk() {
	let asked = std::cell::Cell::new(0);
	let probe = |_: &str| {
		asked.set(asked.get() + 1);
		true
	};
	assert_eq!(choose(Some("fish"), Some("./zsh"), probe), "/bin/sh");
	assert_eq!(asked.get(), 0);
}

#[test]
fn a_path_with_spaces_is_one_path() {
	let spaced = |path: &str| path == "/Applications/My Shell/sh";
	assert_eq!(
		choose(Some("/Applications/My Shell/sh"), None, spaced),
		"/Applications/My Shell/sh"
	);
}

#[test]
fn the_login_shell_is_used_when_the_environment_is_a_relative_name() {
	assert_eq!(choose(Some("bash"), Some("/bin/zsh"), exists), "/bin/zsh");
}

#[test]
fn the_login_shell_that_is_gone_leaves_the_posix_shell() {
	assert_eq!(choose(None, Some("/opt/gone/fish"), exists), "/bin/sh");
}

#[test]
fn the_environment_beats_the_login_shell_even_when_both_exist() {
	assert_eq!(choose(Some("/bin/sh"), Some("/bin/zsh"), exists), "/bin/sh");
}

#[test]
fn a_directory_is_not_a_program() {
	assert!(!is_program("/"));
	assert!(!is_program("/tmp"));
}

#[test]
fn a_missing_path_is_not_a_program() {
	assert!(!is_program("/no/such/shell/anywhere"));
	assert!(!is_program(""));
}

#[test]
fn the_system_shell_is_a_program() {
	assert!(is_program("/bin/sh"));
}

#[test]
fn a_nul_in_the_environment_is_not_a_program() {
	assert!(!is_program("/bin/sh\0x"));
}
