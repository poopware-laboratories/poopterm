//! Putting software-drawn frames on screen without softbuffer: which window
//! buffer each frame goes into and which rows it copies (`pool`, `copy`),
//! then the window system's own shared-memory buffers (`wayland`, `x11`).
pub(crate) mod copy;
#[cfg(test)]
mod copy_tests;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) mod memory;
#[cfg(all(test, any(target_os = "linux", target_os = "freebsd")))]
mod memory_tests;
pub(crate) mod pool;
#[cfg(test)]
mod pool_tests;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) mod wayland;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) mod x11;
