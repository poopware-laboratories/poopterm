//! Safe wrappers over the operating system calls that `std` lacks: the one
//! place outside the C bindings (`gpu::vk`, `present::x11`, …) where calls
//! into the kernel are written, so the rest of the program stays safe code.
pub(crate) mod command;
pub(crate) mod dl;
pub(crate) mod exits;
pub(crate) mod fd;
#[cfg(target_vendor = "apple")]
pub(crate) mod jit;
#[cfg_attr(
	all(target_vendor = "apple", not(test)),
	expect(dead_code, reason = "no window buffers")
)]
pub(crate) mod mem;
pub(crate) mod process;
pub(crate) mod pty;
pub(crate) mod retry;
pub(crate) mod signal;
pub(crate) mod spawn;
pub(crate) mod termios;
pub(crate) mod user;
