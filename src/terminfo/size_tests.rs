//! Keeps the compiled entry small enough for macOS's ncurses 5.7 `tic`.
use std::{fs, process::Command};

use super::{NAME, SOURCE, tests::TempDir, tests::have};

/// ncurses 5 refuses to write a compiled entry larger than this; macOS
/// still ships that `tic`, and the install script fails with "error writing".
const LEGACY_LIMIT: u64 = 4096;

/// Compiles the entry in the legacy format, which ncurses 6 also writes
/// when no number exceeds 16 bits, and returns its size in bytes.
fn legacy_size() -> Option<u64> {
	let dir = TempDir::new("legacy-size");
	let source = SOURCE.replace("pairs#0x10000", "pairs#0x7fff");
	assert_ne!(source, SOURCE, "the entry no longer sets pairs#0x10000");
	let path = dir.0.join("legacy.terminfo");
	fs::write(&path, source).unwrap();
	let status = Command::new("tic")
		.arg("-x")
		.arg("-o")
		.arg(&dir.0)
		.arg(&path)
		.status()
		.ok()?;
	assert!(status.success());
	let compiled = ["x", "78"]
		.iter()
		.map(|sub| dir.0.join(sub).join(NAME))
		.find(|p| p.exists())?;
	let bytes = fs::read(compiled).unwrap();
	// Legacy entries start with the 16-bit magic 0432 (little endian).
	assert_eq!(&bytes[..2], [0x1a, 0x01]);
	Some(bytes.len() as u64)
}

#[test]
fn the_legacy_compiled_entry_fits_old_tic_with_room_to_grow() {
	if !have("tic") {
		return;
	}
	let size = legacy_size().expect("tic wrote no entry");
	// Leave room for one more short capability before the limit bites;
	// tmux's margin capabilities took most of what was left.
	assert!(size <= LEGACY_LIMIT - 64, "{size} bytes");
}
