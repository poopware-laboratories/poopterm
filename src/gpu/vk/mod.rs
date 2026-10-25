//! Our own Vulkan bindings: the loader opened at run time, so Poopterm
//! runs where there is no Vulkan, and only the types and functions the
//! renderer uses, laid out as the specification declares them.
pub(crate) mod call;
#[cfg(test)]
mod call_tests;
pub(crate) mod choose;
#[cfg(test)]
mod choose_tests;
pub(crate) mod device;
#[cfg(test)]
mod device_tests;
#[cfg(test)]
mod driver_tests;
pub(crate) mod entry;
pub(crate) mod features;
#[cfg(test)]
mod features_tests;
pub(crate) mod instance;
pub(crate) mod library;
#[cfg(test)]
mod loader_tests;
pub(crate) mod types;
#[cfg(test)]
mod types_tests;
pub(crate) mod version;
