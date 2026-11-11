//! Who and where we run: the user ID, an account's login shell from the
//! account database, and the host name.
use std::ffi::CStr;

/// This process's real user ID.
pub(crate) fn uid() -> u32 {
	// SAFETY: `getuid` takes no arguments, touches no memory and cannot fail.
	unsafe { libc::getuid() }
}

/// The login shell the account database lists for user `uid` (what `chsh`
/// sets), or None for an unknown user or a lookup error.
pub(crate) fn shell(uid: u32) -> Option<String> {
	let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
	let mut buffer = vec![0u8; 4096];
	let mut found: *mut libc::passwd = std::ptr::null_mut();
	let (into, room) = (buffer.as_mut_ptr().cast(), buffer.len());
	// SAFETY: `entry`, `buffer` and `found` outlive the call and the buffer's
	// length is passed with it; `getpwuid_r` writes only inside them. Never
	// plain `getpwuid`, whose one buffer other threads share.
	let status = unsafe { libc::getpwuid_r(uid, entry.as_mut_ptr(), into, room, &mut found) };
	if status != 0 || found.is_null() {
		return None;
	}
	// SAFETY: a zero status with a non-null `found` means `getpwuid_r` filled
	// in `entry`, which we own; we read it, not the pointer it handed back.
	let shell = unsafe { entry.assume_init_ref() }.pw_shell;
	if shell.is_null() {
		return None;
	}
	// SAFETY: `pw_shell` is a NUL-terminated string inside `buffer`, which is
	// alive here; it is copied before `buffer` is dropped.
	unsafe { CStr::from_ptr(shell) }
		.to_str()
		.ok()
		.map(str::to_owned)
}

/// This machine's host name, or None if the system cannot say.
pub(crate) fn hostname() -> Option<String> {
	// POSIX host names are at most 255 bytes; one more keeps the NUL.
	let mut buffer = [0u8; 256];
	// SAFETY: the buffer is writable for the whole length passed.
	if unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
		return None;
	}
	let end = buffer
		.iter()
		.position(|&byte| byte == 0)
		.unwrap_or(buffer.len());
	Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

#[cfg(test)]
#[path = "user_tests.rs"]
mod tests;
