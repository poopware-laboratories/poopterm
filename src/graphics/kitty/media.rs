//! Where image data comes from (`t=`): the payload itself, a file, a
//! temporary file deleted after reading, or a POSIX shared memory object.
//!
//! Files are read only when they are regular files outside `/proc`, `/sys`
//! and `/dev`, without blocking (FIFOs) and up to [`MAX_TRANSFER`] bytes.
//! Only marked files in temporary directories are deleted.
use std::{
	fs::{File, OpenOptions},
	io::{self, Read, Seek, SeekFrom},
	os::{
		fd::{FromRawFd, OwnedFd},
		unix::fs::OpenOptionsExt,
	},
	path::{Path, PathBuf},
};

use super::{Command, Error, MAX_TRANSFER};

/// Temporary files must contain this in their name to be read and deleted.
const TEMP_MARKER: &str = "tty-graphics-protocol";

/// Returns the image data a command refers to; `payload` is the decoded
/// payload (the data itself, or a path or shared memory name).
pub(crate) fn load(command: &Command, payload: Vec<u8>) -> Result<Vec<u8>, Error> {
	match command.letter(b't', b'd') {
		b'd' => Ok(payload),
		b'f' => read_file(&file_path(&payload)?, command),
		b't' => {
			let path = file_path(&payload)?;
			if !is_temporary(&path) {
				return Err(invalid("not a marked file in a temporary directory"));
			}
			let data = read_file(&path, command);
			// Delete it even when it could not be decoded, as clients expect.
			let _ = std::fs::remove_file(&path);
			data
		}
		b's' => read_shared_memory(&payload, command),
		_ => Err(invalid("unknown transmission medium")),
	}
}

fn invalid(message: &str) -> Error {
	Error::new("EINVAL", message)
}

fn io_error(error: &io::Error) -> Error {
	match error.kind() {
		io::ErrorKind::NotFound => Error::new("ENOENT", "no such file"),
		io::ErrorKind::PermissionDenied => Error::new("EPERM", "permission denied"),
		_ => Error::new("EBADF", error.to_string()),
	}
}

/// An absolute path, with symlinks resolved, outside the special file systems.
fn file_path(payload: &[u8]) -> Result<PathBuf, Error> {
	let path = std::str::from_utf8(payload)
		.ok()
		.map(Path::new)
		.filter(|path| path.is_absolute())
		.ok_or_else(|| invalid("path must be absolute UTF-8"))?;
	let special = |path: &Path| {
		!path.starts_with("/dev/shm")
			&& ["/proc", "/sys", "/dev"]
				.iter()
				.any(|prefix| path.starts_with(prefix))
	};
	if special(path) {
		return Err(invalid("special files are not read"));
	}
	let path = path.canonicalize().map_err(|error| io_error(&error))?;
	if special(&path) {
		return Err(invalid("special files are not read"));
	}
	Ok(path)
}

fn is_temporary(path: &Path) -> bool {
	let marked = path
		.file_name()
		.and_then(|name| name.to_str())
		.is_some_and(|name| name.contains(TEMP_MARKER));
	let directories = [
		std::env::temp_dir(),
		"/tmp".into(),
		"/var/tmp".into(),
		"/dev/shm".into(),
	];
	marked
		&& path.parent().is_some_and(|parent| {
			directories
				.iter()
				.filter_map(|directory| directory.canonicalize().ok())
				.any(|directory| directory == parent)
		})
}

/// The byte range `O`/`S` selects in data of `len` bytes (`S=0`: to the end).
fn range(command: &Command, len: usize) -> Result<(usize, usize), Error> {
	let offset = (command.uint(b'O') as usize).min(len);
	let size = match command.uint(b'S') as usize {
		0 => len - offset,
		size => size.min(len - offset),
	};
	if size > MAX_TRANSFER {
		return Err(Error::new("EFBIG", "image data too large"));
	}
	Ok((offset, size))
}

fn read_file(path: &Path, command: &Command) -> Result<Vec<u8>, Error> {
	// Non-blocking, so a FIFO swapped in after the checks cannot hang us.
	let file = OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NONBLOCK)
		.open(path)
		.map_err(|error| io_error(&error))?;
	let metadata = file.metadata().map_err(|error| io_error(&error))?;
	if !metadata.is_file() {
		return Err(invalid("not a regular file"));
	}
	read_range(file, metadata.len() as usize, command)
}

/// Reads the `O`/`S` range of an open file of `len` bytes.
fn read_range(mut file: File, len: usize, command: &Command) -> Result<Vec<u8>, Error> {
	let (offset, size) = range(command, len)?;
	file
		.seek(SeekFrom::Start(offset as u64))
		.map_err(|error| io_error(&error))?;
	let mut data = Vec::with_capacity(size);
	file
		.take(size as u64)
		.read_to_end(&mut data)
		.map_err(|error| io_error(&error))?;
	Ok(data)
}

fn read_shared_memory(payload: &[u8], command: &Command) -> Result<Vec<u8>, Error> {
	let name = std::str::from_utf8(payload).map_err(|_| invalid("bad shared memory name"))?;
	let name = if name.starts_with('/') {
		name.to_owned()
	} else {
		format!("/{name}")
	};
	if name.len() > 255 || name[1..].contains('/') {
		return Err(invalid("bad shared memory name"));
	}
	let c_name = std::ffi::CString::new(name).map_err(|_| invalid("bad shared memory name"))?;
	// SAFETY: `c_name` is a valid C string; a non-negative result is a new
	// descriptor that the `OwnedFd` closes.
	let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_RDONLY, 0) };
	if fd < 0 {
		return Err(io_error(&io::Error::last_os_error()));
	}
	// SAFETY: as above, `fd` is open and owned by nothing else.
	let file = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
	// The object is consumed: unlink it whatever happens next.
	// SAFETY: `c_name` is a valid C string.
	unsafe { libc::shm_unlink(c_name.as_ptr()) };
	let len = file.metadata().map_err(|error| io_error(&error))?.len() as usize;
	read_shared(file, len, command)
}

/// Linux and FreeBSD read shared memory like a file, so a client shrinking
/// it while we read only shortens the data.
#[cfg(not(target_os = "macos"))]
fn read_shared(file: File, len: usize, command: &Command) -> Result<Vec<u8>, Error> {
	read_range(file, len, command)
}

/// macOS cannot `read` shared memory, so it is mapped.
#[cfg(target_os = "macos")]
fn read_shared(file: File, len: usize, command: &Command) -> Result<Vec<u8>, Error> {
	let (offset, size) = range(command, len)?;
	if size == 0 {
		return Ok(Vec::new());
	}
	let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
	let len = offset + size;
	// SAFETY: a fresh read-only shared mapping of `len` bytes, no more than
	// the open descriptor holds.
	let map = unsafe {
		libc::mmap(
			std::ptr::null_mut(),
			len,
			libc::PROT_READ,
			libc::MAP_SHARED,
			fd,
			0,
		)
	};
	if map == libc::MAP_FAILED {
		return Err(io_error(&io::Error::last_os_error()));
	}
	// SAFETY: the mapping is `len` readable bytes, and stays mapped until
	// the copy below is made.
	let mapped = unsafe { std::slice::from_raw_parts(map.cast::<u8>(), len) };
	let bytes = mapped[offset..].to_vec();
	// SAFETY: unmaps the mapping above; `mapped` is not used again.
	unsafe { libc::munmap(map, len) };
	Ok(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	fn command(control: &str) -> Command {
		Command::parse(control.as_bytes()).unwrap()
	}

	fn code(result: Result<Vec<u8>, Error>) -> &'static str {
		result.unwrap_err().code
	}

	fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
		let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
		std::fs::write(&path, contents).unwrap();
		path
	}

	#[test]
	fn direct_data_is_the_payload() {
		assert_eq!(load(&command("t=d"), b"abc".to_vec()).unwrap(), b"abc");
		assert_eq!(load(&command(""), b"abc".to_vec()).unwrap(), b"abc");
		assert_eq!(code(load(&command("t=x"), Vec::new())), "EINVAL");
	}

	#[test]
	fn files_are_read_with_offset_and_size_and_kept() {
		let path = temp_file("poopterm-kitty-file", b"0123456789");
		let name = path.to_str().unwrap().as_bytes().to_vec();
		assert_eq!(load(&command("t=f"), name.clone()).unwrap(), b"0123456789");
		assert_eq!(load(&command("t=f,O=2,S=3"), name.clone()).unwrap(), b"234");
		assert_eq!(load(&command("t=f,O=8,S=5"), name.clone()).unwrap(), b"89");
		assert!(path.exists());
		std::fs::remove_file(&path).unwrap();
		assert_eq!(code(load(&command("t=f"), name)), "ENOENT");
	}

	#[test]
	fn special_files_and_directories_are_refused() {
		for path in ["/proc/self/environ", "/dev/zero", "/sys/kernel/notes", "/"] {
			let result = load(&command("t=f"), path.as_bytes().to_vec());
			assert_eq!(code(result), "EINVAL", "{path}");
		}
		assert_eq!(code(load(&command("t=f"), b"relative".to_vec())), "EINVAL");
	}

	#[test]
	fn temporary_files_must_be_marked_and_are_deleted() {
		let path = temp_file("tty-graphics-protocol-test", b"pixels");
		let name = path.to_str().unwrap().as_bytes().to_vec();
		assert_eq!(load(&command("t=t"), name).unwrap(), b"pixels");
		assert!(!path.exists());
		// Without the marker in its name, a file is not deleted.
		let unmarked = temp_file("poopterm-unmarked", b"pixels");
		let name = unmarked.to_str().unwrap().as_bytes().to_vec();
		assert_eq!(code(load(&command("t=t"), name)), "EINVAL");
		assert!(unmarked.exists());
		std::fs::remove_file(unmarked).unwrap();
	}

	/// A shared memory object named `name` holding `bytes`, made the way
	/// clients do: macOS cannot `write` shared memory, so it is sized and
	/// mapped.
	fn shared_memory(name: &str, bytes: &[u8]) {
		let c_name = std::ffi::CString::new(name).unwrap();
		// SAFETY: a C string this test owns.
		let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) };
		assert!(fd >= 0);
		let len = bytes.len() as libc::off_t;
		// SAFETY: a descriptor this test owns.
		assert_eq!(unsafe { libc::ftruncate(fd, len) }, 0);
		if bytes.is_empty() {
			// SAFETY: closes the descriptor above, used no more.
			unsafe { libc::close(fd) };
			return;
		}
		let protection = libc::PROT_READ | libc::PROT_WRITE;
		// SAFETY: a new shared mapping of the bytes just sized.
		let map = unsafe {
			libc::mmap(
				std::ptr::null_mut(),
				bytes.len(),
				protection,
				libc::MAP_SHARED,
				fd,
				0,
			)
		};
		assert_ne!(map, libc::MAP_FAILED);
		// SAFETY: the mapping is `bytes.len()` writable bytes and overlaps nothing.
		unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), map.cast(), bytes.len()) };
		// SAFETY: unmaps the mapping above, used no more.
		unsafe { libc::munmap(map, bytes.len()) };
		// SAFETY: closes the descriptor above, used no more.
		unsafe { libc::close(fd) };
	}

	#[test]
	fn shared_memory_is_read_and_unlinked() {
		let name = format!("/poopterm-kitty-{}", std::process::id());
		shared_memory(&name, b"shared");
		assert_eq!(
			load(&command("t=s,S=4"), name.clone().into_bytes()).unwrap(),
			b"shar"
		);
		assert_eq!(code(load(&command("t=s"), name.into_bytes())), "ENOENT");
		assert_eq!(code(load(&command("t=s"), b"/a/b".to_vec())), "EINVAL");
	}

	#[test]
	fn shared_memory_is_read_from_an_offset_and_clamped_at_its_end() {
		let name = format!("/poopterm-kitty-offset-{}", std::process::id());
		for (keys, expected) in [
			("t=s,O=2,S=3", &b"are"[..]),
			("t=s,O=4", b"ed"),
			("t=s,O=5,S=9", b"d"),
		] {
			shared_memory(&name, b"shared");
			let read = load(&command(keys), name.clone().into_bytes()).unwrap();
			assert_eq!(read, expected, "{keys}");
			// Each read consumes the object.
			assert_eq!(
				code(load(&command("t=s"), name.clone().into_bytes())),
				"ENOENT"
			);
		}
	}

	#[test]
	fn empty_shared_memory_and_offsets_at_its_end_read_nothing() {
		let name = format!("/poopterm-kitty-empty-{}", std::process::id());
		shared_memory(&name, b"");
		assert_eq!(
			load(&command("t=s"), name.clone().into_bytes()).unwrap(),
			b""
		);
		shared_memory(&name, b"shared");
		assert_eq!(load(&command("t=s,O=6"), name.into_bytes()).unwrap(), b"");
	}

	#[test]
	fn shared_memory_names_take_a_leading_slash_and_have_a_limit() {
		let name = format!("poopterm-kitty-bare-{}", std::process::id());
		let slashed = format!("/{name}");
		shared_memory(&slashed, b"bare");
		assert_eq!(load(&command("t=s"), name.into_bytes()).unwrap(), b"bare");
		// Both names are one object, unlinked by the read.
		assert_eq!(code(load(&command("t=s"), slashed.into_bytes())), "ENOENT");
		// 255 bytes with the slash is the most a name may have.
		let long = format!("/{}", "x".repeat(255));
		assert_eq!(code(load(&command("t=s"), long.into_bytes())), "EINVAL");
	}
}
