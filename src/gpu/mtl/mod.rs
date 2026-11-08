//! Our own Metal bindings for macOS, reached through the Objective-C
//! runtime: the default device and its queue, shared buffers and managed
//! textures, for the grid renderer to draw with.
pub(crate) mod buffer;
#[cfg(test)]
mod buffer_tests;
pub(crate) mod device;
#[cfg(test)]
pub(crate) mod fake_tests;
pub(crate) mod texture;
