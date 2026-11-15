//! Keeps the small entry small enough for macOS's `tic`: ncurses 6.0 on
//! macOS 27 (5.7 before macOS 14), which refuses compiled entries over
//! 4096 bytes. The full entry is for ncurses 6.1 and later.
use std::{fs, process::Command};

use super::{NAME, SMALL, tests::TempDir, tests::have};

/// ncurses before 6.1 refuses to write a compiled entry larger than this
/// ("error writing"); macOS still ships such a `tic`.
const LEGACY_LIMIT: u64 = 4096;

/// Compiles the entry in the legacy format, which ncurses 6 also writes
/// when no number exceeds 16 bits, and returns its size in bytes.
fn legacy_size() -> Option<u64> {
	let dir = TempDir::new("legacy-size");
	let source = SMALL.replace("pairs#0x10000", "pairs#0x7fff");
	assert_ne!(source, SMALL, "the entry no longer sets pairs#0x10000");
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
fn the_small_entry_fits_old_tic_with_room_to_grow() {
	if !have("tic") {
		return;
	}
	let size = legacy_size().expect("tic wrote no entry");
	// Leave room for one more short capability before the limit bites;
	// tmux's margin capabilities took most of what was left.
	assert!(size <= LEGACY_LIMIT - 64, "{size} bytes");
}
