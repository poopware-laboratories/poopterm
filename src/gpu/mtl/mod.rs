//! Our own Metal renderer for macOS, reached through the Objective-C
//! runtime: the default device and its queue, shared buffers, managed
//! textures, the quad pipeline compiled from MSL at run time, command
//! buffers, and the grid renderer over the backend-free data in `gpu`.
// Messages still share blocks; the Metal renderer work wraps each in its
// own safe method and removes this.
#![expect(clippy::multiple_unsafe_ops_per_block)]
pub(crate) mod buffer;
#[cfg(test)]
mod buffer_tests;
pub(crate) mod commands;
pub(crate) mod device;
#[cfg(test)]
pub(crate) mod fake_commands_tests;
#[cfg(test)]
pub(crate) mod fake_tests;
pub(crate) mod pipeline;
pub(crate) mod renderer;
#[cfg(test)]
mod renderer_gpu_tests;
pub(crate) mod texture;
pub(crate) mod window;
