//! Our own Vulkan bindings: the loader opened at run time, so Poopterm
//! runs where there is no Vulkan, and only the types and functions the
//! renderer uses, laid out as the specification declares them.
pub(crate) mod buffer;
#[cfg(test)]
mod buffer_tests;
pub(crate) mod call;
#[cfg(test)]
mod call_tests;
pub(crate) mod choose;
#[cfg(test)]
mod choose_tests;
pub(crate) mod commands;
#[cfg(test)]
mod commands_tests;
pub(crate) mod device;
#[cfg(test)]
mod device_tests;
#[cfg(test)]
mod driver_tests;
pub(crate) mod entry;
pub(crate) mod features;
#[cfg(test)]
mod features_tests;
pub(crate) mod image;
#[cfg(test)]
mod image_tests;
pub(crate) mod instance;
pub(crate) mod library;
#[cfg(test)]
mod loader_tests;
pub(crate) mod memory;
#[cfg(test)]
mod memory_driver_tests;
#[cfg(test)]
mod memory_tests;
pub(crate) mod pipeline;
#[cfg(test)]
mod pipeline_driver_tests;
#[cfg(test)]
mod pipeline_tests;
pub(crate) mod pipeline_types;
#[cfg(test)]
mod pipeline_types_tests;
pub(crate) mod recording;
#[cfg(test)]
mod recording_tests;
#[cfg(test)]
mod round_trip_tests;
pub(crate) mod shader;
#[cfg(test)]
mod shader_tests;
pub(crate) mod submit_types;
#[cfg(test)]
mod submit_types_tests;
pub(crate) mod timeline;
#[cfg(test)]
mod timeline_tests;
pub(crate) mod types;
#[cfg(test)]
mod types_tests;
pub(crate) mod version;
