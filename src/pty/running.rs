//! Whether a program other than the shell has the terminal: the job in the
//! foreground, as `tcgetpgrp` reports it, is not the shell's own group.
//! Closing a tab then ends that program too, so the app asks first.

/// What names a program whose name the system does not give.
pub(crate) const UNNAMED: &str = "A program";

/// The name of the job in the foreground when it is not the shell's
/// (`shell`, which leads its own group); `None` while the shell has the
/// terminal, or when either is unknown, so a tab whose state cannot be
/// read closes as before.
pub(crate) fn running_program(
	shell: Option<u32>,
	foreground: Option<i32>,
	name: impl FnOnce(i32) -> Option<String>,
) -> Option<String> {
	let (shell, group) = (i32::try_from(shell?).ok()?, foreground?);
	(group != shell).then(|| name(group).unwrap_or_else(|| UNNAMED.to_owned()))
}
