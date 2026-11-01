use std::{
	fs,
	io::ErrorKind,
	path::{Path, PathBuf},
};

use super::{
	BUNDLED, Font,
	data::{Data, read_file},
	fallback::SystemFallback,
	fallback_tests::{HAN, font_with_only, installed},
	load_font,
};

fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-data-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir
}

fn bundled_file(dir: &Path) -> PathBuf {
	let path = dir.join("bundled.ttf");
	fs::write(&path, BUNDLED).unwrap();
	path
}

#[test]
fn every_kind_of_data_gives_back_its_bytes() {
	let owned: Data = b"owned".to_vec().into();
	let built_in: Data = (b"static" as &'static [u8]).into();
	assert!(matches!(owned, Data::Owned(_)));
	assert!(matches!(built_in, Data::Static(_)));
	assert_eq!(&*owned, b"owned");
	assert_eq!(&*built_in, b"static");
}

#[test]
fn a_font_file_is_mapped_rather_than_read() {
	let dir = scratch("mapped");
	let data = read_file(&bundled_file(&dir)).unwrap();
	assert!(matches!(data, Data::Mapped(_)), "mapped");
	assert_eq!(&*data, BUNDLED);
}

#[test]
fn a_missing_font_file_is_an_error() {
	let dir = scratch("missing");
	let error = read_file(&dir.join("gone.ttf")).err().expect("missing");
	assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn a_directory_is_an_error() {
	let dir = scratch("directory");
	assert!(read_file(&dir).is_err());
}

#[test]
fn an_empty_file_reads_as_empty_bytes_which_no_font_parses() {
	let dir = scratch("empty");
	let path = dir.join("empty.ttf");
	fs::write(&path, b"").unwrap();
	let data = read_file(&path).unwrap();
	assert!(data.is_empty());
	assert!(Font::from_bytes(data, 0).is_err());
}

#[cfg(unix)]
#[test]
fn a_fifo_is_an_error_and_not_waited_on() {
	let dir = scratch("fifo");
	let path = dir.join("font.ttf");
	let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
	// SAFETY: `name` is a C string naming a path in our scratch directory.
	assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
	// Reading a FIFO waits for a writer: the test would hang.
	assert!(read_file(&path).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn files_that_cannot_be_mapped_are_read_instead() {
	// procfs reports a size of 0, so mapping is refused; reading still works.
	let data = read_file(Path::new("/proc/self/status")).unwrap();
	assert!(matches!(data, Data::Owned(_)), "read onto the heap");
	assert!(
		data.starts_with(b"Name:"),
		"{:?}",
		&data[..data.len().min(16)]
	);
}

#[test]
fn a_mapped_font_draws_exactly_like_the_same_bytes_on_the_heap() {
	let dir = scratch("same");
	let mapped = Font::from_bytes(read_file(&bundled_file(&dir)).unwrap(), 0).unwrap();
	let heap = Font::from_bytes(BUNDLED.to_vec(), 0).unwrap();
	assert!(mapped.is_mapped());
	assert!(!heap.is_mapped());
	assert_eq!(
		mapped.horizontal_line_metrics(17.0),
		heap.horizontal_line_metrics(17.0)
	);
	for ch in ['A', 'g', '@', '─', '█', '漢'] {
		assert_eq!(
			mapped.lookup_glyph_index(ch),
			heap.lookup_glyph_index(ch),
			"{ch}"
		);
		assert_eq!(mapped.rasterize(ch, 17.0), heap.rasterize(ch, 17.0), "{ch}");
	}
}

#[test]
fn the_configured_font_is_mapped() {
	let dir = scratch("configured");
	let font = load_font(&bundled_file(&dir), 0).unwrap();
	assert!(font.is_mapped());
	assert_ne!(font.lookup_glyph_index('A'), 0);
}

#[test]
fn a_configured_font_that_is_missing_reports_why() {
	let dir = scratch("configured-missing");
	let error = load_font(&dir.join("gone.ttf"), 0).err().expect("missing");
	assert!(error.contains("No such file"), "{error}");
}

#[test]
fn system_fallback_fonts_are_mapped() {
	let (dir, fonts) = installed("data-mapped", &[font_with_only('漢')], &[HAN]);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('漢', false, &mut loaded), Some(0));
	assert!(loaded[0].is_mapped());
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_bundled_font_stays_in_the_binary_without_a_copy() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	assert!(!font.is_mapped());
	assert!(font.is_static());
}

#[test]
fn an_unreadable_font_file_is_an_error_not_read_another_way() {
	use std::os::unix::fs::PermissionsExt;
	let dir = scratch("unreadable");
	let path = bundled_file(&dir);
	fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();
	// SAFETY: geteuid takes no arguments and cannot fail.
	if unsafe { libc::geteuid() } == 0 {
		return; // root reads any file whatever its mode
	}
	let error = read_file(&path).err().expect("unreadable");
	assert_eq!(error.kind(), ErrorKind::PermissionDenied);
}

#[test]
fn truncated_and_garbage_font_files_are_refused_without_panicking() {
	let dir = scratch("broken");
	let cases: [(&str, &[u8]); 4] = [
		("half.ttf", &BUNDLED[..BUNDLED.len() / 2]),
		("header.ttf", &BUNDLED[..12]),
		("text.ttf", b"this is not a font at all"),
		("zeros.ttf", &[0; 8192]),
	];
	for (name, bytes) in cases {
		let path = dir.join(name);
		fs::write(&path, bytes).unwrap();
		let data = read_file(&path).unwrap();
		assert!(matches!(data, Data::Mapped(_)), "{name} is mapped");
		if let Ok(font) = Font::from_bytes(data, 0) {
			// A face whose tables still parse must draw without reading past its end.
			let _ = font.rasterize('A', 17.0);
		}
	}
	assert!(load_font(&dir.join("text.ttf"), 0).is_err());
	assert!(load_font(&dir.join("zeros.ttf"), 0).is_err());
}

#[test]
fn a_font_keeps_drawing_after_its_file_is_deleted() {
	// A package upgrade or uninstall while the terminal runs.
	let dir = scratch("deleted");
	let path = bundled_file(&dir);
	let font = load_font(&path, 0).unwrap();
	let before = font.rasterize('g', 21.0);
	fs::remove_file(&path).unwrap();
	assert_eq!(font.rasterize('g', 21.0), before);
	assert_ne!(font.lookup_glyph_index('g'), 0);
}

#[test]
fn a_font_keeps_its_own_glyphs_after_its_file_is_replaced() {
	let dir = scratch("replaced");
	let path = bundled_file(&dir);
	let font = load_font(&path, 0).unwrap();
	let new = dir.join("bundled.ttf.new");
	fs::write(&new, font_with_only('漢')).unwrap();
	fs::rename(&new, &path).unwrap();
	assert_ne!(font.lookup_glyph_index('A'), 0, "still the old font");
	let reloaded = load_font(&path, 0).unwrap();
	assert_eq!(
		reloaded.lookup_glyph_index('A'),
		0,
		"the new file has only 漢"
	);
	assert_ne!(reloaded.lookup_glyph_index('漢'), 0);
}

#[test]
fn a_font_reached_through_a_symlink_is_mapped() {
	let dir = scratch("symlink");
	let path = bundled_file(&dir);
	std::os::unix::fs::symlink(&path, dir.join("link.ttf")).unwrap();
	let font = load_font(&dir.join("link.ttf"), 0).unwrap();
	assert!(font.is_mapped());
}
