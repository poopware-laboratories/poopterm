//! System clipboard and URL opening through platform helper programs.
use crate::term::clipboard::{ClipboardWrite, Selection};
#[cfg(all(unix, not(target_os = "macos")))]
use std::env;
use std::{
	io::Write,
	process::{Command, Stdio},
	thread,
};

pub(crate) fn clipboard_write_with(
	program: &str,
	args: &[&str],
	text: &str,
) -> std::io::Result<()> {
	write_bytes_with(program, args, text.as_bytes())
}

fn write_bytes_with(program: &str, args: &[&str], data: &[u8]) -> std::io::Result<()> {
	let mut child = Command::new(program)
		.args(args)
		.stdin(Stdio::piped())
		.spawn()?;
	child.stdin.take().unwrap().write_all(data)?;
	let status = child.wait()?;
	if status.success() {
		Ok(())
	} else {
		Err(std::io::Error::other(format!(
			"{program} exited with {status}"
		)))
	}
}

pub(crate) fn clipboard_read_with(program: &str, args: &[&str]) -> std::io::Result<String> {
	let output = Command::new(program).args(args).output()?;
	if output.status.success() {
		Ok(String::from_utf8_lossy(&output.stdout).into_owned())
	} else {
		Err(std::io::Error::other(format!(
			"{program} exited with {}",
			output.status
		)))
	}
}

#[cfg(target_os = "macos")]
pub(crate) fn open_external(target: &str) -> std::io::Result<()> {
	let mut child = Command::new("open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn open_external(target: &str) -> std::io::Result<()> {
	let mut child = Command::new("xdg-open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_write(text: &str) -> std::io::Result<()> {
	clipboard_write_with("pbcopy", &[], text)
}

#[cfg(target_os = "macos")]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	clipboard_read_with("pbpaste", &[])
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn clipboard_write(text: &str) -> std::io::Result<()> {
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(()) = clipboard_write_with("wl-copy", &[], text)
	{
		return Ok(());
	}
	clipboard_write_with("xclip", &["-selection", "clipboard"], text)
		.or_else(|_| clipboard_write_with("xsel", &["--clipboard", "--input"], text))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(text) = clipboard_read_with("wl-paste", &["--no-newline"])
	{
		return Ok(text);
	}
	clipboard_read_with("xclip", &["-selection", "clipboard", "-o"])
		.or_else(|_| clipboard_read_with("xsel", &["--clipboard", "--output"]))
}

#[cfg(not(unix))]
pub(crate) fn clipboard_write(_: &str) -> std::io::Result<()> {
	Err(std::io::Error::other(
		"clipboard support is unavailable on this platform",
	))
}

#[cfg(not(unix))]
pub(crate) fn clipboard_read() -> std::io::Result<String> {
	Err(std::io::Error::other(
		"clipboard support is unavailable on this platform",
	))
}

/// Performs a clipboard write a program requested (OSC 52 or 5522) on a
/// helper thread, so a slow helper program never stalls the UI.
pub(crate) fn write_from_program(write: ClipboardWrite) {
	thread::spawn(move || {
		if let Err(error) = write_selection(&write) {
			crate::warnings::warning!("clipboard write from a program failed: {error}");
		}
	});
}

/// Helper programs take one type per write: plain text when offered, else
/// the first type.
fn preferred(write: &ClipboardWrite) -> Option<(&str, &[u8])> {
	let text = |mime: &str| mime == "text/plain" || mime.starts_with("text/plain;");
	write
		.items
		.iter()
		.find(|(mime, _)| text(mime))
		.or(write.items.first())
		.map(|(mime, data)| (mime.as_str(), &data[..]))
}

#[cfg(target_os = "macos")]
fn write_selection(write: &ClipboardWrite) -> std::io::Result<()> {
	match preferred(write) {
		// macOS has no primary selection; pbcopy takes text only.
		Some((mime, data))
			if write.selection == Selection::Clipboard && mime.starts_with("text/plain") =>
		{
			write_bytes_with("pbcopy", &[], data)
		}
		Some(_) => Err(std::io::Error::other(
			"only plain text can be copied on macOS",
		)),
		None => Ok(()),
	}
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_selection(write: &ClipboardWrite) -> std::io::Result<()> {
	let Some((mime, data)) = preferred(write) else {
		return Ok(());
	};
	let primary = write.selection == Selection::Primary;
	if env::var_os("WAYLAND_DISPLAY").is_some() {
		let mut args = vec!["--type", mime];
		if primary {
			args.push("--primary");
		}
		if let Ok(()) = write_bytes_with("wl-copy", &args, data) {
			return Ok(());
		}
	}
	let selection = if primary { "primary" } else { "clipboard" };
	write_bytes_with("xclip", &["-selection", selection, "-t", mime], data).or_else(|error| {
		if mime.starts_with("text/plain") {
			let selection = if primary { "--primary" } else { "--clipboard" };
			write_bytes_with("xsel", &[selection, "--input"], data)
		} else {
			Err(error)
		}
	})
}

#[cfg(not(unix))]
fn write_selection(_: &ClipboardWrite) -> std::io::Result<()> {
	Err(std::io::Error::other(
		"clipboard support is unavailable on this platform",
	))
}
