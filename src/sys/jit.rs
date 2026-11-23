//! Executable memory for code the app loads itself (`jit::loader`), and
//! the JIT a debugger grants on iOS.
//!
//! iOS runs only signed code, unless a debugger is attached
//! (`CS_DEBUGGED`): then memory written by the app may be made executable.
//! Before iOS 26 that is all it takes (`Arena::plain`). On iOS 26 and later
//! the Trusted Execution Monitor (TXM) also requires the debugger to
//! prepare each executable region: StikDebug's universal script answers a
//! `brk #0xf00d` with the command in x16 (1: prepare x1 bytes at x0, or
//! anywhere when x0 is 0, the region's address back in x0; 0: detach).
//! The app then writes code through a second, writable mapping of the same
//! pages (`Arena::prepared`). A `brk` no debugger script answers ends the
//! app, so `prepare_region` is only called after the app asked StikDebug
//! for the script and saw the debugger attach.
use std::io;

/// `CS_DEBUGGED`, of `<kern/cs_blobs.h>`: a debugger is attached and the
/// kernel lets the process run code it wrote.
const CS_DEBUGGED: u32 = 0x1000_0000;
const CS_OPS_STATUS: u32 = 0;

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
	fn csops(pid: libc::pid_t, ops: u32, useraddr: *mut libc::c_void, usersize: usize)
	-> libc::c_int;
	fn sys_icache_invalidate(start: *mut libc::c_void, len: usize);
	#[cfg_attr(
		all(not(target_os = "ios"), not(test)),
		expect(dead_code, reason = "only iOS devices prepare regions")
	)]
	static mach_task_self_: u32;
	#[cfg_attr(
		all(not(target_os = "ios"), not(test)),
		expect(dead_code, reason = "only iOS devices prepare regions")
	)]
	fn mach_vm_remap(
		target: u32,
		address: *mut u64,
		size: u64,
		mask: u64,
		flags: libc::c_int,
		source_task: u32,
		source_address: u64,
		copy: libc::c_int,
		current: *mut libc::c_int,
		maximum: *mut libc::c_int,
		inherit: u32,
	) -> libc::c_int;
	#[cfg_attr(
		all(not(target_os = "ios"), not(test)),
		expect(dead_code, reason = "only iOS devices prepare regions")
	)]
	fn mach_vm_protect(
		task: u32,
		address: u64,
		size: u64,
		set_maximum: libc::c_int,
		protection: libc::c_int,
	) -> libc::c_int;
}

/// The process's code-signing flags, or 0 where there are none.
pub(crate) fn code_signing_flags() -> u32 {
	#[cfg(target_vendor = "apple")]
	{
		let mut flags: u32 = 0;
		// CS_OPS_STATUS writes one u32 into `flags`, whose size is passed.
		let pid = std::process::id() as libc::pid_t;
		// SAFETY: as above.
		let failed = unsafe { csops(pid, CS_OPS_STATUS, (&raw mut flags).cast(), 4) };
		if failed == 0 {
			return flags;
		}
	}
	0
}

/// Whether a debugger has given this process JIT.
pub(crate) fn debugged() -> bool {
	code_signing_flags() & CS_DEBUGGED != 0
}

/// The system's version, as `kern.osproductversion` says it ("27.2.1").
pub(crate) fn os_version() -> Option<String> {
	let mut buf = [0u8; 32];
	let mut len = buf.len();
	// SAFETY: writes at most `len` bytes into `buf` and the length used into
	// `len`; the name is a NUL-terminated C string.
	let failed = unsafe {
		libc::sysctlbyname(
			c"kern.osproductversion".as_ptr(),
			buf.as_mut_ptr().cast(),
			&mut len,
			std::ptr::null_mut(),
			0,
		)
	};
	if failed != 0 {
		return None;
	}
	let text = buf.get(..len)?;
	let end = text
		.iter()
		.position(|&byte| byte == 0)
		.unwrap_or(text.len());
	String::from_utf8(text[..end].to_vec()).ok()
}

/// Asks the debugger's script for `len` executable bytes: their address.
///
/// # Safety
///
/// A debugger running StikDebug's universal script must be attached, or
/// the breakpoint ends the process.
#[cfg(all(target_os = "ios", target_arch = "aarch64"))]
pub(crate) unsafe fn prepare_region(len: usize) -> usize {
	let mut address: usize = 0;
	// SAFETY: the caller vouches a debugger answers the breakpoint: it
	// reads x16 and x1, prepares the region, writes its address to x0 and
	// steps past the instruction. No memory or stack is touched here.
	unsafe {
		std::arch::asm!(
			"brk #0xf00d",
			inout("x0") address,
			in("x1") len,
			in("x16") 1usize,
			options(nostack),
		);
	}
	address
}

/// Tells the debugger's script it may detach: no more regions.
///
/// # Safety
///
/// As `prepare_region`.
#[cfg(all(target_os = "ios", target_arch = "aarch64"))]
pub(crate) unsafe fn detach_debugger() {
	// SAFETY: as in `prepare_region`, with the detach command.
	unsafe {
		std::arch::asm!("brk #0xf00d", in("x16") 0usize, lateout("x0") _, options(nostack));
	}
}

/// Rounds `len` up to whole pages, or down to the last whole page when
/// that would overflow.
pub(crate) fn pages(len: usize) -> usize {
	const PAGE: usize = 16384;
	len
		.checked_next_multiple_of(PAGE)
		.unwrap_or(usize::MAX - usize::MAX % PAGE)
}

/// How an arena makes code executable.
#[derive(Debug)]
enum Kind {
	/// Each image is mapped writable, then its code made executable.
	Plain,
	/// A region the debugger made executable, written through a writable
	/// mirror at `mirror`; data pages are replaced by plain writable ones.
	#[cfg_attr(
		all(not(target_os = "ios"), not(test)),
		expect(dead_code, reason = "only iOS devices prepare regions")
	)]
	Prepared { mirror: usize },
}

/// Memory images are loaded into, handed out in page-aligned pieces and
/// never given back: code may still be pointed to after its program ends.
#[derive(Debug)]
pub(crate) struct Arena {
	start: usize,
	len: usize,
	used: usize,
	kind: Kind,
}

impl Arena {
	/// An arena that maps each image itself (`len` bytes at most in all).
	pub(crate) fn plain(len: usize) -> Self {
		Self {
			start: 0,
			len,
			used: 0,
			kind: Kind::Plain,
		}
	}

	/// An arena over `len` bytes at `start` that a debugger made executable,
	/// with a writable mirror made of the same pages.
	///
	/// # Safety
	///
	/// `start..start + len` is an executable mapping of this process that
	/// nothing else uses.
	#[cfg_attr(
		all(not(target_os = "ios"), not(test)),
		expect(dead_code, reason = "only iOS devices prepare regions")
	)]
	pub(crate) unsafe fn prepared(start: usize, len: usize) -> io::Result<Self> {
		let mut mirror: u64 = 0;
		let (mut current, mut maximum) = (0, 0);
		// SAFETY: maps the same pages again, anywhere, shared (copy 0); every
		// pointer is to a local that outlives the call.
		let failed = unsafe {
			mach_vm_remap(
				mach_task_self_,
				&mut mirror,
				len as u64,
				0,
				1, // VM_FLAGS_ANYWHERE
				mach_task_self_,
				start as u64,
				0,
				&mut current,
				&mut maximum,
				2, // VM_INHERIT_NONE
			)
		};
		if failed != 0 {
			return Err(io::Error::other(format!("mach_vm_remap failed ({failed})")));
		}
		// SAFETY: makes the new mirror, which only we know of, readable and
		// writable, never executable.
		let failed = unsafe {
			mach_vm_protect(
				mach_task_self_,
				mirror,
				len as u64,
				0,
				libc::PROT_READ | libc::PROT_WRITE,
			)
		};
		if failed != 0 {
			return Err(io::Error::other(format!(
				"mach_vm_protect failed ({failed})"
			)));
		}
		Ok(Self {
			start,
			len,
			used: 0,
			kind: Kind::Prepared {
				mirror: usize::try_from(mirror).map_err(io::Error::other)?,
			},
		})
	}

	/// Bytes handed out and in all.
	pub(crate) fn usage(&self) -> (usize, usize) {
		(self.used, self.len)
	}

	/// Whether code goes in a region a debugger prepared.
	pub(crate) fn is_prepared(&self) -> bool {
		matches!(self.kind, Kind::Prepared { .. })
	}

	/// Room for an image of `len` bytes: where it will run. Writable until
	/// `seal`.
	pub(crate) fn reserve(&mut self, len: usize) -> io::Result<Image> {
		let len = pages(len);
		if len == 0 || self.used.saturating_add(len) > self.len {
			return Err(io::Error::other(
				"no room left for code: restart Poopterm and enable JIT again",
			));
		}
		let image = match self.kind {
			Kind::Plain => {
				// SAFETY: a new private anonymous mapping, anywhere; checked below.
				let address = unsafe {
					libc::mmap(
						std::ptr::null_mut(),
						len,
						libc::PROT_READ | libc::PROT_WRITE,
						libc::MAP_PRIVATE | libc::MAP_ANON,
						-1,
						0,
					)
				};
				if address == libc::MAP_FAILED {
					return Err(io::Error::last_os_error());
				}
				Image {
					address: address as usize,
					len,
					mirror: None,
				}
			}
			Kind::Prepared { mirror } => Image {
				address: self.start + self.used,
				len,
				mirror: Some(mirror + self.used),
			},
		};
		self.used += len;
		Ok(image)
	}
}

/// The memory of one image, at `address`.
#[derive(Debug)]
pub(crate) struct Image {
	address: usize,
	len: usize,
	/// Where its bytes can be written while its code is executable.
	mirror: Option<usize>,
}

impl Image {
	pub(crate) fn address(&self) -> usize {
		self.address
	}

	/// Puts `bytes` (fixed up for `address`) in place, with the pieces in
	/// `code` (offset, length) executable and the rest writable data.
	pub(crate) fn seal(&self, bytes: &[u8], code: &[(usize, usize)]) -> io::Result<()> {
		if bytes.len() > self.len
			|| code
				.iter()
				.any(|&(at, len)| at.saturating_add(len) > self.len)
		{
			return Err(io::Error::other("an image bigger than its room"));
		}
		match self.mirror {
			None => {
				// SAFETY: the mapping is ours, writable and `self.len` bytes long,
				// and `bytes` fits in it.
				unsafe {
					std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.address as *mut u8, bytes.len())
				};
				for &(at, len) in code {
					protect(self.address + at, len, libc::PROT_READ | libc::PROT_EXEC)?;
				}
			}
			Some(mirror) => {
				// Data first: the executable pages outside `code` are replaced by
				// writable ones at the same addresses.
				let mut data_at = 0;
				let mut pieces: Vec<(usize, usize)> = code.to_vec();
				pieces.sort_unstable();
				pieces.push((self.len, 0));
				for (at, len) in pieces {
					if at > data_at {
						remap_writable(self.address + data_at, at - data_at)?;
						let end = at.min(bytes.len());
						if end > data_at {
							// SAFETY: just mapped writable, inside the image's room.
							unsafe {
								std::ptr::copy_nonoverlapping(
									bytes[data_at..end].as_ptr(),
									(self.address + data_at) as *mut u8,
									end - data_at,
								);
							}
						}
					}
					let end = at.saturating_add(len).min(bytes.len());
					if end > at {
						// SAFETY: the mirror maps the same pages writable, and the
						// piece is inside the image's room.
						unsafe {
							std::ptr::copy_nonoverlapping(
								bytes[at..end].as_ptr(),
								(mirror + at) as *mut u8,
								end - at,
							)
						};
					}
					data_at = at.saturating_add(len);
				}
			}
		}
		for &(at, len) in code {
			flush_instructions(self.address + at, len);
		}
		Ok(())
	}
}

fn protect(address: usize, len: usize, protection: libc::c_int) -> io::Result<()> {
	// SAFETY: changes the protection of pages of an image mapping we own.
	if unsafe { libc::mprotect(address as *mut libc::c_void, pages(len), protection) } != 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}

/// Replaces `len` bytes of pages at `address` by fresh writable ones.
fn remap_writable(address: usize, len: usize) -> io::Result<()> {
	// SAFETY: MAP_FIXED over pages of our own arena that hold no code yet.
	let mapped = unsafe {
		libc::mmap(
			address as *mut libc::c_void,
			len,
			libc::PROT_READ | libc::PROT_WRITE,
			libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_FIXED,
			-1,
			0,
		)
	};
	if mapped == libc::MAP_FAILED {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}

/// Makes the CPU fetch the instructions just written.
fn flush_instructions(address: usize, len: usize) {
	#[cfg(target_vendor = "apple")]
	// SAFETY: invalidates the instruction cache over our own mapped image.
	unsafe {
		sys_icache_invalidate(address as *mut libc::c_void, len)
	};
	#[cfg(not(target_vendor = "apple"))]
	let _ = (address, len);
}

#[cfg(test)]
#[path = "jit_tests.rs"]
mod tests;
