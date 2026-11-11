//! Safe wrappers over the operating system calls that `std` lacks: the one
//! place outside the C bindings (`gpu::vk`, `present::x11`, …) where calls
//! into the kernel are written, so the rest of the program stays safe code.
pub(crate) mod exits;
#[cfg_attr(
	all(target_os = "macos", not(test)),
	expect(dead_code, reason = "no window buffers")
)]
pub(crate) mod mem;
pub(crate) mod process;
pub(crate) mod pty;
pub(crate) mod retry;
pub(crate) mod spawn;
pub(crate) mod termios;
pub(crate) mod user;
