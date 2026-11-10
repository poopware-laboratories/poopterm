//! Runs the shipped hooks in real shells (skipped when a shell is missing).
use std::process::{Command, Stdio};

/// A shell command in its own session, without a controlling terminal: an
/// interactive shell (`-i`) opens `/dev/tty` and, from a background process
/// group, stops on SIGTTIN and hangs the test, as on CI's FreeBSD VM.
pub(super) fn shell_command(shell: &str) -> Command {
	use std::os::unix::process::CommandExt;
	unsafe extern "C" {
		safe fn setsid() -> i32;
	}
	let mut command = Command::new(shell);
	command.stdin(Stdio::null());
	// SAFETY: setsid is async-signal-safe and touches no parent state.
	unsafe {
		command.pre_exec(|| {
			setsid();
			Ok(())
		});
	}
	command
}

/// Runs `script` in `shell`, or returns `None` when the shell is not installed.
pub(super) fn run(shell: &str, args: &[&str], script: &str) -> Option<(String, String)> {
	let output = shell_command(shell).args(args).arg(script).output().ok()?;
	Some((
		String::from_utf8_lossy(&output.stdout).into_owned(),
		String::from_utf8_lossy(&output.stderr).into_owned(),
	))
}

#[test]
fn zsh_hooks_report_exit_status_and_directory_without_errors() {
	let script = concat!(
		"source shell-integration/zsh.zsh; cd /tmp; false; _poopterm_precmd; ",
		"print -rn -- \"[$PATH]\""
	);
	let Some((stdout, stderr)) = run("zsh", &["-fc"], script) else {
		return;
	};
	assert_eq!(stderr, "");
	assert!(stdout.contains("\x1b]133;D;1\x07"), "{stdout:?}");
	assert!(stdout.contains("/tmp\x07"), "{stdout:?}");
	assert!(!stdout.contains("[/tmp]"), "hook must not clobber $PATH");
}

#[test]
fn bash_marks_command_input_after_the_prompt() {
	let script = concat!(
		"PS1='P> '; source shell-integration/bash.sh; printf '%s|' \"$PS1\"; ",
		"false; __poopterm_prompt_command"
	);
	let Some((stdout, stderr)) = run("bash", &["--norc", "-c"], script) else {
		return;
	};
	assert_eq!(stderr, "");
	let (ps1, prompt) = stdout.split_once('|').unwrap();
	assert_eq!(ps1, "P> \\[\\e]133;B\\a\\]");
	assert!(prompt.starts_with("\x1b]133;D;1\x07"), "{prompt:?}");
}

/// A directory whose name contains BEL, ESC and other bytes outside the
/// URI-safe set. Raw control bytes in OSC 7 would end the sequence early
/// and let the rest of the name run as terminal commands.
struct HostileDirectory {
	root: std::path::PathBuf,
	path: std::path::PathBuf,
}

impl HostileDirectory {
	fn new(shell: &str) -> Self {
		let root = std::env::temp_dir().join(format!("poopterm-hooks-{}-{shell}", std::process::id()));
		let path = root.join("a b\x07\x1b]52;c;eA==\x07é%");
		std::fs::create_dir_all(&path).unwrap();
		Self { root, path }
	}
}

impl Drop for HostileDirectory {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

const ENCODED_TAIL: &str = "/a%20b%07%1B%5D52%3Bc%3BeA%3D%3D%07%C3%A9%25";

#[test]
fn zsh_percent_encodes_unsafe_directory_bytes() {
	let dir = HostileDirectory::new("zsh");
	let script = "source shell-integration/zsh.zsh; cd -- \"$TARGET\"; _poopterm_precmd";
	let Some(output) = shell_command("zsh")
		.args(["-fc", script])
		.env("TARGET", &dir.path)
		.output()
		.ok()
	else {
		return;
	};
	let stdout = String::from_utf8_lossy(&output.stdout);
	let osc7 = stdout.split("\x1b]7;").nth(1).unwrap_or_default();
	assert!(osc7.ends_with(&format!("{ENCODED_TAIL}\x07")), "{stdout:?}");
	assert_eq!(osc7.matches('\x07').count(), 1, "{stdout:?}");
}

#[test]
fn bash_percent_encodes_unsafe_directory_bytes() {
	let dir = HostileDirectory::new("bash");
	let script = "source shell-integration/bash.sh; cd -- \"$TARGET\"; __poopterm_prompt_command";
	let Some(output) = shell_command("bash")
		.args(["--norc", "-c", script])
		.env("TARGET", &dir.path)
		.output()
		.ok()
	else {
		return;
	};
	let stdout = String::from_utf8_lossy(&output.stdout);
	let osc7 = stdout.split("\x1b]7;").nth(1).unwrap_or_default();
	assert!(osc7.ends_with(&format!("{ENCODED_TAIL}\x07")), "{stdout:?}");
	assert_eq!(osc7.matches('\x07').count(), 1, "{stdout:?}");
}

/// A scratch directory removed on drop.
struct Scratch(std::path::PathBuf);

impl Scratch {
	fn new(name: &str) -> Self {
		let dir = std::env::temp_dir().join(format!("poopterm-inject-{}-{name}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		Self(dir)
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// A fake `ssh` on PATH that prints the TERM it was started with.
fn fake_ssh(dir: &std::path::Path) -> std::ffi::OsString {
	let bin = dir.join("bin");
	std::fs::create_dir_all(&bin).unwrap();
	let ssh = bin.join("ssh");
	std::fs::write(&ssh, "#!/bin/sh\necho \"ssh-term=$TERM\"\n").unwrap();
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
	}
	let mut path = bin.into_os_string();
	path.push(":");
	path.push(std::env::var_os("PATH").unwrap_or_default());
	path
}

#[test]
fn ssh_uses_a_common_term_while_term_is_poopterm() {
	let scratch = Scratch::new("ssh");
	let path = fake_ssh(&scratch.0);
	for (shell, args, script) in [
		(
			"zsh",
			&["-fc"][..],
			"source shell-integration/zsh.zsh; ssh host",
		),
		(
			"bash",
			&["--norc", "-c"][..],
			"source shell-integration/bash.sh; ssh host",
		),
	] {
		let run = |term: &str, extra: &str| {
			// The shell reports ssh busy when another thread forked while
			// fake_ssh wrote it; that run says nothing, so run it again.
			crate::busy_tests::retry_busy(|| {
				let output = shell_command(shell)
					.args(args)
					.arg(format!("{extra}{script}"))
					.env("TERM", term)
					.env("PATH", &path)
					.output()?;
				if crate::busy_tests::busy_in(&output.stderr) {
					return Err(std::io::ErrorKind::ExecutableFileBusy.into());
				}
				Ok(output)
			})
			.ok()
			.map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
		};
		let Some(wrapped) = run("xterm-poopterm", "") else {
			continue;
		};
		assert!(
			wrapped.contains("ssh-term=xterm-256color"),
			"{shell}: {wrapped:?}"
		);
		let other = run("screen-256color", "").unwrap();
		assert!(
			other.contains("ssh-term=screen-256color"),
			"{shell}: {other:?}"
		);
		let custom = run("xterm-poopterm", "ssh() { echo custom; }; ").unwrap();
		assert!(
			custom.contains("custom"),
			"{shell} keeps a user's ssh function: {custom:?}"
		);
	}
}

#[test]
fn zsh_injection_loads_hooks_and_restores_the_users_startup_files() {
	use super::inject::{Shell, injection_env, install_scripts};
	if Command::new("zsh").arg("--version").output().is_err() {
		return;
	}
	let scratch = Scratch::new("zsh");
	let scripts = scratch.0.join("scripts");
	install_scripts(&scripts).unwrap();
	let home = scratch.0.join("home");
	let custom = scratch.0.join("custom");
	for dir in [&home, &custom] {
		std::fs::create_dir_all(dir).unwrap();
		std::fs::write(dir.join(".zshenv"), format!("ENV_FROM={}\n", dir.display())).unwrap();
		std::fs::write(dir.join(".zshrc"), format!("RC_FROM={}\n", dir.display())).unwrap();
	}
	let probe = "print -r -- \"$+functions[_poopterm_precmd] ${ZDOTDIR-unset} $RC_FROM $ENV_FROM\"";
	for original in [None, Some(&custom)] {
		let mut command = shell_command("zsh");
		command
			.args(["-i", "-c", probe])
			.env("HOME", &home)
			.env_remove("ZDOTDIR");
		if let Some(dir) = original {
			command.env("ZDOTDIR", dir);
		}
		let lookup = |name: &str| {
			(name == "ZDOTDIR")
				.then(|| original.map(|dir| dir.clone().into_os_string()))
				.flatten()
		};
		for (name, value) in injection_env(Shell::Zsh, &scripts, lookup) {
			command.env(name, value);
		}
		let output = command.output().unwrap();
		let stdout = String::from_utf8_lossy(&output.stdout);
		let expected_dir = original.map_or("unset".to_owned(), |d| d.display().to_string());
		let user_dir = original.unwrap_or(&home);
		assert_eq!(
			stdout.trim(),
			format!("1 {expected_dir} {0} {0}", user_dir.display()),
			"stderr: {}",
			String::from_utf8_lossy(&output.stderr)
		);
	}
}

#[test]
fn bash_injection_loads_profiles_and_hooks_once_and_respects_off() {
	use super::inject::{Shell, injection_env, install_scripts, shell_args};
	if Command::new("bash").arg("--version").output().is_err() {
		return;
	}
	let scratch = Scratch::new("bash-auto");
	let scripts = scratch.0.join("scripts with spaces");
	let home = scratch.0.join("home");
	std::fs::create_dir_all(&home).unwrap();
	std::fs::write(
		home.join(".bash_profile"),
		"PROFILE=yes; source ~/.bashrc\n",
	)
	.unwrap();
	std::fs::write(
		home.join(".bashrc"),
		"RC_COUNT=$(( ${RC_COUNT:-0} + 1 )); PS1='P> '\n",
	)
	.unwrap();
	install_scripts(&scripts).unwrap();
	let env = injection_env(Shell::Bash, &scripts, |_| None);
	// Without a terminal, interactive bash checks who owns its stderr. On
	// FreeBSD a pipe reports process group 0, so bash stops itself with
	// SIGTTIN as if in the background; a file fails the check instead.
	let stderr = scratch.0.join("stderr");
	let output = shell_command("bash")
		.args(shell_args(Shell::Bash, &env))
		.args(["-c", "printf 'probe=%s:%s:%s:%s' \"$PROFILE\" \"$RC_COUNT\" \"$__POOPTERM_INTEGRATION_LOADED\" \"${POOPTERM_BASH_RCFILE-unset}\""])
		.env("HOME", &home)
		.envs(env)
		.stderr(std::fs::File::create(&stderr).unwrap())
		.output().unwrap();
	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = std::fs::read_to_string(stderr).unwrap();
	assert!(
		stdout.contains("probe=yes:1:1:unset"),
		"{stdout:?}, stderr: {stderr}"
	);
	assert_eq!(
		shell_args(Shell::Bash, &[]),
		[std::ffi::OsString::from("-l")]
	);
	assert!(super::inject::prepare("/bin/bash", Some("off")).is_empty());
}

/// The command line a hook reported with OSC 133 C, decoded.
pub(super) fn reported_command(stdout: &str) -> Option<String> {
	let value = stdout.split("\x1b]133;C;cmdline_url=").nth(1)?;
	super::command_line::percent_decode(value.split('\x07').next()?)
}

pub(super) const COMMAND: &str = "echo 'a;b' \u{e9}\x1b]2;x\x07";

#[test]
fn zsh_reports_command_lines_and_click_to_move() {
	let script = "source shell-integration/zsh.zsh; _poopterm_precmd; _poopterm_preexec \"$COMMAND\"";
	let Some(output) = shell_command("zsh")
		.args(["-fc", script])
		.env("COMMAND", COMMAND)
		.output()
		.ok()
	else {
		return;
	};
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(stdout.contains("\x1b]133;A;cl=line\x07"), "{stdout:?}");
	assert_eq!(
		reported_command(&stdout).as_deref(),
		Some(COMMAND),
		"{stdout:?}"
	);
}

#[test]
fn fish_reports_command_lines_and_click_to_move() {
	let script =
		"source shell-integration/fish.fish; __poopterm_prompt; __poopterm_preexec \"$COMMAND\"";
	let Some(args) = super::fish_tests::without_native_marks() else {
		return;
	};
	let Some(output) = shell_command("fish")
		.args(args)
		.args(["-c", script])
		.env("COMMAND", COMMAND)
		.output()
		.ok()
	else {
		return;
	};
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(stdout.contains("\x1b]133;A;cl=line\x07"), "{stdout:?}");
	assert_eq!(
		reported_command(&stdout).as_deref(),
		Some(COMMAND),
		"{stdout:?}"
	);
}

#[test]
fn bash_asks_for_click_to_move() {
	let script = "source shell-integration/bash.sh; __poopterm_prompt_command";
	let Some((stdout, _)) = run("bash", &["--norc", "-c"], script) else {
		return;
	};
	assert!(stdout.contains("\x1b]133;A;cl=line\x07"), "{stdout:?}");
}
