//! Tests of memory mappings and shared memory objects: what a mapping
//! shows, who sees writes through it, what is refused, and the life of
//! named objects.
use super::*;
use std::{
	ffi::CString,
	fs::{File, OpenOptions},
	io::ErrorKind,
	os::{fd::AsFd, unix::fs::FileExt},
	path::PathBuf,
};

/// A file holding `bytes`, removed when the returned guard drops.
struct Temp(PathBuf);

impl Drop for Temp {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.0);
	}
}

fn temp(name: &str, bytes: &[u8]) -> (Temp, File) {
	let path = std::env::temp_dir().join(format!("poopterm-mem-{}-{name}", std::process::id()));
	std::fs::write(&path, bytes).unwrap();
	let file = OpenOptions::new()
		.read(true)
		.write(true)
		.open(&path)
		.unwrap();
	(Temp(path), file)
}

/// A shared memory name unique to this process and test.
fn shm_name(test: &str) -> CString {
	CString::new(format!("/poopterm-mem-{}-{test}", std::process::id())).unwrap()
}

#[test]
fn a_read_mapping_shows_the_file() {
	let (_temp, file) = temp("read", b"glyph outlines");
	for shared in [false, true] {
		let map = Map::read(file.as_fd(), 14, shared).unwrap();
		assert_eq!(map.bytes(), b"glyph outlines", "shared: {shared}");
	}
}

#[test]
fn a_mapping_may_cover_only_the_start() {
	let (_temp, file) = temp("start", b"0123456789");
	let map = Map::read(file.as_fd(), 4, false).unwrap();
	assert_eq!(map.bytes(), b"0123");
}

#[test]
fn an_empty_mapping_is_refused() {
	let (_temp, file) = temp("empty", b"");
	for result in [
		Map::read(file.as_fd(), 0, false),
		Map::shared(file.as_fd(), 0),
	] {
		assert_eq!(
			result.err().map(|error| error.kind()),
			Some(ErrorKind::InvalidInput)
		);
	}
}

#[test]
fn a_directory_cannot_be_mapped() {
	let dir = File::open(std::env::temp_dir()).unwrap();
	assert!(Map::read(dir.as_fd(), 4096, false).is_err());
}

#[test]
fn a_file_open_only_to_write_cannot_be_mapped() {
	let (temp, _file) = temp("write-only", b"data");
	let file = OpenOptions::new().write(true).open(&temp.0).unwrap();
	let refused = Map::read(file.as_fd(), 4, false)
		.err()
		.map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}

#[test]
fn a_file_open_only_to_read_cannot_be_mapped_to_write() {
	let (temp, _file) = temp("read-only", b"data");
	let file = File::open(&temp.0).unwrap();
	let refused = Map::shared(file.as_fd(), 4).err().map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}

#[test]
fn a_read_mapping_cannot_be_written() {
	let (_temp, file) = temp("no-write", b"data");
	let mut map = Map::read(file.as_fd(), 4, true).unwrap();
	assert!(map.bytes_mut().is_none());
	assert!(map.words_mut().is_none());
}

#[test]
fn writes_through_a_shared_mapping_reach_the_file() {
	let (_temp, file) = temp("shared", b"........");
	let mut map = Map::shared(file.as_fd(), 8).unwrap();
	map.bytes_mut().unwrap()[2..5].copy_from_slice(b"abc");
	let mut read = [0; 8];
	file.read_exact_at(&mut read, 0).unwrap();
	assert_eq!(&read, b"..abc...");
}

#[test]
fn a_shared_mapping_sees_later_writes_to_the_file_and_other_mappings() {
	let (_temp, file) = temp("visibility", b"before");
	let shared = Map::read(file.as_fd(), 6, true).unwrap();
	file.write_all_at(b"after!", 0).unwrap();
	assert_eq!(shared.bytes(), b"after!");
	let mut other = Map::shared(file.as_fd(), 6).unwrap();
	other.bytes_mut().unwrap()[0] = b'A';
	assert_eq!(shared.bytes(), b"After!");
}

#[test]
fn words_are_the_whole_words_of_the_mapping() {
	let (_temp, file) = temp("words", &[0; 10]);
	let mut map = Map::shared(file.as_fd(), 10).unwrap();
	let words = map.words_mut().unwrap();
	assert_eq!(
		words.len(),
		2,
		"the two bytes past the last word are left out"
	);
	words[1] = u32::from_ne_bytes(*b"wxyz");
	assert_eq!(&map.bytes()[4..8], b"wxyz");
	assert_eq!(&map.bytes()[8..], [0, 0]);
}

#[test]
fn mappings_outlive_the_descriptor_they_came_from() {
	let (_temp, file) = temp("outlive", b"kept");
	let map = Map::read(file.as_fd(), 4, false).unwrap();
	drop(file);
	assert_eq!(map.bytes(), b"kept");
}

#[test]
fn maps_move_between_threads() {
	let (_temp, file) = temp("threads", b"moved");
	let map = Map::read(file.as_fd(), 5, false).unwrap();
	let bytes = std::thread::spawn(move || map.bytes().to_vec())
		.join()
		.unwrap();
	assert_eq!(bytes, b"moved");
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn anonymous_objects_start_empty_and_are_closed_on_exec() {
	use std::os::fd::AsRawFd;
	let fd = anonymous(c"poopterm-test").unwrap();
	let file = File::from(fd);
	assert_eq!(file.metadata().unwrap().len(), 0);
	// SAFETY: reads the flags of a descriptor this test owns.
	let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
	assert_eq!(flags & libc::FD_CLOEXEC, libc::FD_CLOEXEC);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn each_anonymous_object_is_its_own() {
	let first = File::from(anonymous(c"poopterm-test").unwrap());
	let second = File::from(anonymous(c"poopterm-test").unwrap());
	first.set_len(4).unwrap();
	second.set_len(4).unwrap();
	let mut one = Map::shared(first.as_fd(), 4).unwrap();
	let two = Map::shared(second.as_fd(), 4).unwrap();
	one.bytes_mut().unwrap().copy_from_slice(b"mine");
	assert_eq!(two.bytes(), [0; 4]);
}

#[test]
fn a_named_object_is_shared_until_unlinked() {
	let name = shm_name("named");
	let _ = shm_unlink(&name);
	let writer = File::from(shm_open(&name, true).unwrap());
	writer.set_len(5).unwrap();
	Map::shared(writer.as_fd(), 5)
		.unwrap()
		.bytes_mut()
		.unwrap()
		.copy_from_slice(b"hello");
	let reader = shm_open(&name, false).unwrap();
	shm_unlink(&name).unwrap();
	// The name is gone, but the open object and its contents are not.
	assert_eq!(
		Map::read(reader.as_fd(), 5, true).unwrap().bytes(),
		b"hello"
	);
	let gone = shm_open(&name, false).err().map(|error| error.kind());
	assert_eq!(gone, Some(ErrorKind::NotFound));
}

#[test]
fn opening_a_missing_object_to_read_does_not_create_it() {
	let name = shm_name("missing");
	let _ = shm_unlink(&name);
	let missing = shm_open(&name, false).err().map(|error| error.kind());
	assert_eq!(missing, Some(ErrorKind::NotFound));
	assert_eq!(
		shm_unlink(&name).err().map(|error| error.kind()),
		Some(ErrorKind::NotFound)
	);
}

#[test]
fn an_object_opened_to_read_cannot_be_mapped_to_write() {
	let name = shm_name("read-only");
	let _ = shm_unlink(&name);
	let writer = File::from(shm_open(&name, true).unwrap());
	writer.set_len(4).unwrap();
	let reader = shm_open(&name, false).unwrap();
	shm_unlink(&name).unwrap();
	let refused = Map::shared(reader.as_fd(), 4)
		.err()
		.map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}
