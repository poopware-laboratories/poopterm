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
