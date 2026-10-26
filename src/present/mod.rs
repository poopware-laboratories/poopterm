//! Putting software-drawn frames on screen without softbuffer: which window
//! buffer each frame goes into and which rows it copies (`pool`, `copy`),
//! then the window system's own shared-memory buffers.
pub(crate) mod copy;
#[cfg(test)]
mod copy_tests;
pub(crate) mod pool;
#[cfg(test)]
mod pool_tests;
