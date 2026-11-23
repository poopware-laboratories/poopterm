//! The Mach-O reader against libraries the system compiler builds, and
//! against damaged ones.
use std::{fs, path::PathBuf, process::Command, sync::OnceLock};

use super::*;

/// A library with what loading must handle: pointers between its own
/// data (rebases), calls into the system's (binds), a weak import, a
/// constructor and an exported entry.
pub(crate) const SOURCE: &str = r#"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
extern int poopterm_not_anywhere(void) __attribute__((weak_import));
static const char *greeting = "hello";
static const char **pointer = &greeting;
static int constructed;
static int counter;
__attribute__((constructor)) static void construct(void) { constructed = 42; }
int poopterm_test_entry(int value) {
	counter++;
	if (value == 1) return (int)strlen(*pointer);
	if (value == 2) return constructed;
	if (value == 3) return poopterm_not_anywhere == NULL;
	if (value == 4) return counter;
	if (value == 5) { char buf[16]; snprintf(buf, sizeof buf, "%d", 1234); return atoi(buf); }
	return -1;
}
"#;

/// The library built from `SOURCE` (with `flags`), once per flag set.
/// Libraries built so far, by the flags they were built with.
type Built = std::sync::Mutex<Vec<(&'static [&'static str], PathBuf)>>;

pub(crate) fn built(flags: &'static [&'static str]) -> Option<PathBuf> {
	static BUILT: OnceLock<Built> = OnceLock::new();
	let built = BUILT.get_or_init(Default::default);
	let mut built = built.lock().unwrap();
	if let Some((_, path)) = built.iter().find(|(made, _)| *made == flags) {
		return Some(path.clone());
	}
	if !cfg!(target_vendor = "apple") {
		return None;
	}
	let dir = std::env::temp_dir().join(format!("poopterm-macho-{}", std::process::id()));
	fs::create_dir_all(&dir).unwrap();
	let source = dir.join("lib.c");
	fs::write(&source, SOURCE).unwrap();
	let out = dir.join(format!("lib{}.dylib", built.len()));
	let made = Command::new("cc")
		.args(["-dynamiclib", "-O0", "-w", "-mmacosx-version-min=13.0"])
		.args(["-Wl,-U,_poopterm_not_anywhere", "-o"])
		.arg(&out)
		.arg(&source)
		.args(flags)
		.output()
		.ok()?;
	assert!(
		made.status.success(),
		"cc failed: {}",
		String::from_utf8_lossy(&made.stderr)
	);
	built.push((flags, out.clone()));
	Some(out)
}

fn library() -> Option<(Vec<u8>, Library)> {
	let bytes = fs::read(built(&[])?).unwrap();
	let library = parse(&bytes).unwrap();
	Some((bytes, library))
}

#[test]
fn a_built_library_reads_as_its_segments_dylibs_and_initializers() {
	let Some((_, library)) = library() else {
		return;
	};
	let names: Vec<_> = library
		.segments
		.iter()
		.map(|segment| segment.name.as_str())
		.collect();
	assert_eq!(names.first(), Some(&"__TEXT"), "{names:?}");
	assert!(names.contains(&"__LINKEDIT"));
	let text = &library.segments[0];
	assert_eq!((text.offset, text.executable()), (0, true));
	assert!(
		library
			.segments
			.iter()
			.any(|segment| !segment.executable() && segment.protection & 2 != 0)
	);
	assert!(
		library
			.dylibs
			.iter()
			.any(|dylib| dylib.contains("libSystem")),
		"{:?}",
		library.dylibs
	);
	assert_eq!(library.initializers.len(), 1, "one constructor section");
	let end = library
		.segments
		.iter()
		.map(|s| s.offset + s.size)
		.max()
		.unwrap();
	assert_eq!(library.span, end);
}

#[test]
fn the_layout_puts_each_segments_bytes_at_its_offset() {
	let Some((bytes, library)) = library() else {
		return;
	};
	let image = layout(&bytes, &library).unwrap();
	assert_eq!(image.len() as u64, library.span);
	assert_eq!(
		&image[..4],
		&MAGIC_64.to_le_bytes(),
		"the header leads __TEXT"
	);
	for segment in &library.segments {
		let (at, from, size) = (
			segment.offset as usize,
			segment.file_offset as usize,
			segment.file_size as usize,
		);
		assert_eq!(
			&image[at..at + size],
			&bytes[from..from + size],
			"{}",
			segment.name
		);
	}
}

#[test]
fn fixups_rebase_its_own_pointers_and_bind_the_systems() {
	let Some((bytes, library)) = library() else {
		return;
	};
	let image = layout(&bytes, &library).unwrap();
	let (imports, fixups) = fixups(&bytes, &library, &image).unwrap();
	let names: Vec<_> = imports.iter().map(|import| import.name.as_str()).collect();
	for wanted in ["_strlen", "___snprintf_chk", "_atoi"] {
		assert!(names.contains(&wanted), "{names:?}");
	}
	let weak = imports
		.iter()
		.find(|import| import.name == "_poopterm_not_anywhere")
		.unwrap();
	assert!(weak.weak);
	assert!(
		imports.iter().all(|import| import.ordinal != 0),
		"all from libSystem or flat"
	);
	assert_eq!(weak.ordinal, -2, "left undefined: looked up anywhere");
	let rebases = fixups
		.iter()
		.filter(|fixup| matches!(fixup, Fixup::Rebase { .. }))
		.count();
	assert!(rebases >= 2, "greeting and pointer: {fixups:?}");
	for fixup in &fixups {
		let (Fixup::Rebase { at, .. } | Fixup::Bind { at, .. }) = *fixup;
		assert!(at + 8 <= library.span);
		if let Fixup::Rebase { target, .. } = *fixup {
			assert!(target < library.span, "rebases point inside the image");
		}
	}
}

#[test]
fn the_entry_is_found_by_its_exported_name_only() {
	let Some((bytes, library)) = library() else {
		return;
	};
	let entry = symbol(&bytes, &library, "_poopterm_test_entry").unwrap();
	let text = &library.segments[0];
	assert!(entry > 0 && entry < text.size, "in __TEXT");
	assert_eq!(
		symbol(&bytes, &library, "_construct"),
		None,
		"static: not exported"
	);
	assert_eq!(symbol(&bytes, &library, "_missing"), None);
}

#[test]
fn applying_writes_rebases_from_the_load_address_and_binds_resolved() {
	let mut image = vec![0u8; 32];
	let fixups = [
		Fixup::Rebase {
			at: 0,
			target: 0x10,
			high: 0,
		},
		Fixup::Rebase {
			at: 8,
			target: 0x18,
			high: 0xab,
		},
		Fixup::Bind {
			at: 16,
			import: 1,
			addend: 4,
		},
		Fixup::Bind {
			at: 24,
			import: 0,
			addend: 0,
		},
	];
	apply(&mut image, &fixups, 0x1_0000, &[0, 0x7000]).unwrap();
	let word = |n: usize| u64::from_le_bytes(image[n * 8..n * 8 + 8].try_into().unwrap());
	assert_eq!(word(0), 0x1_0010);
	assert_eq!(word(1), 0xab00_0000_0001_0018, "the high byte is kept");
	assert_eq!(word(2), 0x7004);
	assert_eq!(word(3), 0, "a missing weak import is null");
	assert!(
		apply(
			&mut image,
			&[Fixup::Bind {
				at: 0,
				import: 2,
				addend: 0
			}],
			0,
			&[0]
		)
		.is_err()
	);
	assert!(
		apply(
			&mut image,
			&[Fixup::Rebase {
				at: 30,
				target: 0,
				high: 0
			}],
			0,
			&[]
		)
		.is_err()
	);
}

#[test]
fn chain_pointers_decode_as_rebases_or_binds() {
	let bind = 1u64 << 63 | 3 << 24 | 2;
	assert_eq!(
		decode(bind, 8, DYLD_CHAINED_PTR_64, 0, 3),
		Ok(Fixup::Bind {
			at: 8,
			import: 2,
			addend: 3
		})
	);
	assert!(
		decode(bind, 8, DYLD_CHAINED_PTR_64, 0, 2).is_err(),
		"import out of the table"
	);
	let rebase = 0x12u64 << 36 | 0x4000_1000;
	assert_eq!(
		decode(rebase, 0, DYLD_CHAINED_PTR_64, 0x4000_0000, 0),
		Ok(Fixup::Rebase {
			at: 0,
			target: 0x1000,
			high: 0x12
		}),
		"PTR_64 names an address"
	);
	assert_eq!(
		decode(0x1000, 0, DYLD_CHAINED_PTR_64_OFFSET, 0x4000_0000, 0),
		Ok(Fixup::Rebase {
			at: 0,
			target: 0x1000,
			high: 0
		}),
		"PTR_64_OFFSET an offset"
	);
	assert!(
		decode(0x10, 0, DYLD_CHAINED_PTR_64, 0x4000_0000, 0).is_err(),
		"below the image"
	);
}

#[test]
fn damaged_and_foreign_files_are_refused_with_a_reason() {
	assert!(parse(b"").is_err());
	assert!(parse(b"#!/bin/sh\necho hi\n").is_err());
	assert!(
		parse(&FAT_MAGIC.to_be_bytes())
			.unwrap_err()
			.contains("universal")
	);
	let mut other_cpu = vec![0u8; 64];
	other_cpu[..4].copy_from_slice(&MAGIC_64.to_le_bytes());
	other_cpu[4..8].copy_from_slice(&(HOST_CPU ^ 1).to_le_bytes());
	assert!(parse(&other_cpu).unwrap_err().contains("CPU"));
	let Some((bytes, _)) = library() else {
		return;
	};
	// Every cut of a real library fails cleanly or still parses: no panic.
	for cut in (0..bytes.len().min(16384)).step_by(97) {
		let _ = parse(&bytes[..cut]).and_then(|library| {
			let image = layout(&bytes[..cut], &library)?;
			fixups(&bytes[..cut], &library, &image)
		});
	}
	// A program is not a library.
	let mut program = bytes.clone();
	program[12..16].copy_from_slice(&2u32.to_le_bytes());
	assert!(
		parse(&program)
			.unwrap_err()
			.contains("not a dynamic library")
	);
}

#[test]
fn a_library_linked_the_old_way_is_refused() {
	let Some(old) = built(&["-Wl,-no_fixup_chains"]) else {
		return;
	};
	let error = parse(&fs::read(old).unwrap()).unwrap_err();
	assert!(error.contains("iOS 13.4"), "{error}");
}
