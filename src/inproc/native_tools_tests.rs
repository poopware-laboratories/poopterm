//! The everyday tools packaged with the app (xz, zstd, file, nnn), run
//! through the in-process shell when `ios/packages/build.sh macosx` has
//! built them, and checked against Python's own codecs and the system's
//! `file`.
use std::{fs, path::Path, process::Command};

use super::{device_in, ran, real_shell};
use crate::inproc::shell::Shell;

/// A shell with the package `name` on `$PATH` (before the bundled
/// commands), its home, and the package's folder.
fn shell_with(name: &str) -> Option<(Shell, std::path::PathBuf, std::path::PathBuf)> {
	let dir = Path::new(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/target/ios-packages/work-macosx"
	))
	.join(name);
	if !dir.join("bin").join(name).is_file() {
		eprintln!("skipped: run ios/packages/build.sh macosx {name}");
		return None;
	}
	let (mut sh, home) = real_shell(name)?;
	let path = format!("{}:{}", dir.join("bin").display(), sh.env["PATH"]);
	sh.env.insert("PATH".into(), path);
	Some((sh, home, dir))
}

/// What Python's `module` makes of `file`: `decompress`ed, as hex.
fn python_decompress(module: &str, file: &Path) -> Option<String> {
	let script = format!(
		"import sys, {module} as m; sys.stdout.write(m.decompress(open(sys.argv[1], 'rb').read()).hex())"
	);
	let out = Command::new("python3")
		.args(["-I", "-c", &script])
		.arg(file)
		.output()
		.ok()?;
	out
		.status
		.success()
		.then(|| String::from_utf8(out.stdout).unwrap())
}

/// Bytes no compressor shrinks to nothing: text that repeats, then noise.
fn sample() -> Vec<u8> {
	let mut bytes = b"the quick brown fox\n".repeat(500);
	let mut state = 0x2545_f491_u32;
	bytes.extend((0..4096).map(|_| {
		state ^= state << 13;
		state ^= state >> 17;
		state ^= state << 5;
		state as u8
	}));
	bytes
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) fn xz_compresses_what_python_reads_and_reads_it_back() {
	let Some((mut sh, home, _)) = shell_with("xz") else {
		return;
	};
	let data = sample();
	fs::write(home.join("data"), &data).unwrap();
	assert_eq!(ran(&mut sh, "xz -k data; echo status $?"), "status 0\n");
	let packed = fs::read(home.join("data.xz")).unwrap();
	assert!(packed.starts_with(b"\xfd7zXZ\0"), "the .xz magic");
	assert!(
		packed.len() < data.len() / 2,
		"it compressed: {}",
		packed.len()
	);
	if let Some(unpacked) = python_decompress("lzma", &home.join("data.xz")) {
		assert_eq!(unpacked, hex(&data), "Python reads it back");
	}
	assert_eq!(ran(&mut sh, "xz -t data.xz; echo status $?"), "status 0\n");
	fs::remove_file(home.join("data")).unwrap();
	assert_eq!(ran(&mut sh, "xz -d data.xz; echo status $?"), "status 0\n");
	assert_eq!(fs::read(home.join("data")).unwrap(), data, "round trip");
	assert!(!home.join("data.xz").exists(), "-d without -k removes it");
	assert_eq!(
		ran(&mut sh, "echo hello | xz -9e | xz -dc"),
		"hello\n",
		"through a pipe, the strongest preset"
	);
	fs::write(home.join("bad.xz"), b"not xz at all").unwrap();
	let bad = ran(&mut sh, "xz -t bad.xz; echo status $?");
	assert!(bad.contains("File format not recognized"), "{bad}");
	assert!(bad.ends_with("status 1\n"), "{bad}");
}

pub(super) fn zstd_compresses_what_python_reads_and_reads_it_back() {
	let Some((mut sh, home, _)) = shell_with("zstd") else {
		return;
	};
	let data = sample();
	fs::write(home.join("data"), &data).unwrap();
	assert_eq!(
		ran(&mut sh, "zstd -q -19 data; echo status $?"),
		"status 0\n"
	);
	let packed = fs::read(home.join("data.zst")).unwrap();
	assert!(packed.starts_with(b"\x28\xb5\x2f\xfd"), "the zstd magic");
	assert!(
		packed.len() < data.len() / 2,
		"it compressed: {}",
		packed.len()
	);
	if let Some(unpacked) = python_decompress("compression.zstd", &home.join("data.zst")) {
		assert_eq!(unpacked, hex(&data), "Python reads it back");
	}
	assert_eq!(
		ran(&mut sh, "zstd -q -d -c data.zst | wc -c | tr -d ' '"),
		format!("{}\n", data.len())
	);
	assert_eq!(ran(&mut sh, "echo hi | zstd -q | zstd -q -d"), "hi\n");
	let bad = ran(&mut sh, "echo nope | zstd -q -d; echo status $?");
	assert!(bad.ends_with("status 1\n"), "not zstd data: {bad}");
}

/// What the system's `file` says of `name` in `dir`, briefly.
fn system_file(dir: &Path, name: &str) -> String {
	let out = Command::new("/usr/bin/file")
		.args(["-b", name])
		.current_dir(dir)
		.output()
		.unwrap();
	String::from_utf8(out.stdout).unwrap()
}

pub(super) fn file_names_kinds_as_the_systems_does() {
	let Some((mut sh, home, dir)) = shell_with("file") else {
		return;
	};
	let magic = dir.join("share/file/magic.mgc");
	assert!(magic.is_file(), "the database is packaged");
	let without = ran(&mut sh, "file -b /etc/hosts; echo status $?");
	assert!(
		without.contains("/usr/share/file/magic"),
		"without MAGIC it looks where a Mac keeps it: {without}"
	);
	sh.env.insert("MAGIC".into(), magic.display().to_string());
	fs::write(home.join("note.txt"), "plain words\n").unwrap();
	fs::write(home.join("empty"), "").unwrap();
	fs::write(home.join("run.sh"), "#!/bin/sh\necho hi\n").unwrap();
	fs::write(
		home.join("page.html"),
		"<!DOCTYPE html>\n<html><body>hi</body></html>\n",
	)
	.unwrap();
	fs::write(home.join("data.json"), "{\"a\": [1, 2, 3]}\n").unwrap();
	fs::write(
		home.join("pixel.png"),
		b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89",
	)
	.unwrap();
	fs::write(home.join("packed.gz"), b"\x1f\x8b\x08\0\0\0\0\0\0\x03").unwrap();
	fs::create_dir(home.join("folder")).unwrap();
	for name in ["note.txt", "empty", "pixel.png", "folder"] {
		assert_eq!(
			ran(&mut sh, &format!("file -b {name}")),
			system_file(&home, name),
			"{name}"
		);
	}
	// Worded differently by the older file macOS has (5.41).
	assert_eq!(ran(&mut sh, "file -b data.json"), "JSON text data\n");
	assert_eq!(
		ran(&mut sh, "file -b run.sh"),
		"POSIX shell script, ASCII text executable\n"
	);
	assert!(ran(&mut sh, "file -b page.html").starts_with("HTML document"));
	assert!(ran(&mut sh, "file -b packed.gz").starts_with("gzip compressed data"));
	assert_eq!(ran(&mut sh, "file --mime-type -b pixel.png"), "image/png\n");
	assert_eq!(
		ran(&mut sh, "file note.txt"),
		"note.txt: ASCII text\n",
		"named, as file prints them"
	);
	let missing = ran(&mut sh, "file -E missing; echo status $?");
	assert!(missing.contains("No such file or directory"), "{missing}");
	assert!(missing.ends_with("status 1\n"), "{missing}");
}

pub(super) fn nnn_lists_a_folder_and_quits() {
	let Some((sh, home, dir)) = shell_with("nnn") else {
		return;
	};
	fs::write(home.join("alpha.txt"), "a").unwrap();
	fs::create_dir(home.join("beta")).unwrap();
	let terminfo = super::toys::terminfo("nnn");
	let bin = dir.join("bin").display().to_string();
	let keys: &[&[u8]] = &[b"nnn\r", b"q", b"echo after nnn\r", b"exit\r"];
	let shown = device_in(sh, Some(&bin), Some(&terminfo), keys);
	assert!(shown.contains("alpha.txt"), "it listed the files");
	assert!(shown.contains("beta"), "and the folder");
	assert!(shown.contains("after nnn\r\n"), "q quit it");
}

pub(super) fn vim_edits_and_saves_a_file_on_the_device() {
	let Some((mut sh, home, dir)) = shell_with("vim") else {
		return;
	};
	let runtime = dir.join("share/vim");
	assert!(
		runtime.join("syntax/c.vim").is_file(),
		"its syntax files are packaged"
	);
	sh.env
		.insert("VIMRUNTIME".into(), runtime.display().to_string());
	fs::write(home.join("note.txt"), "first line\n").unwrap();
	// Ex mode, without a terminal: what scripts use.
	assert_eq!(
		ran(
			&mut sh,
			"vim -Nu NONE -es -c '%s/first/only/' -c 'wq' note.txt; echo status $?"
		),
		"status 0\n"
	);
	assert_eq!(
		fs::read_to_string(home.join("note.txt")).unwrap(),
		"only line\n"
	);
	let version = ran(&mut sh, "vim --version | head -1");
	assert!(version.starts_with("VIM - Vi IMproved 9.1"), "{version}");
	let terminfo = super::toys::terminfo("vim");
	let bin = dir.join("bin").display().to_string();
	let keys: &[&[u8]] = &[
		b"vim note.txt\r",
		b"Goadded by vim\x1b",
		b":syntax on\r",
		b":wq\r",
		b"cat note.txt\r",
		b"exit\r",
	];
	let shown = device_in(sh, Some(&bin), Some(&terminfo), keys);
	assert!(shown.contains("\x1b[?1049h"), "on the alternate screen");
	assert!(
		shown.contains("cat note.txt\r\nonly line\r\nadded by vim\r\n"),
		"what was typed was saved: {:?}",
		&shown[shown.len().saturating_sub(300)..]
	);
	assert!(!shown.contains("E484"), "the syntax files were found");
}
