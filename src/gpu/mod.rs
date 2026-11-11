//! Data for the GPU renderers, kept free of GPU calls so it is tested like
//! any other code: glyph outlines laid out for coverage shaders and stored
//! in a curve buffer, the atlas that holds bitmaps, and the grid as quads;
//! and our own Vulkan and Metal bindings.
pub(crate) mod atlas;
#[cfg(test)]
mod atlas_growth_tests;
#[cfg(test)]
mod atlas_tests;
#[cfg(test)]
mod atlas_usage_tests;
pub(crate) mod bands;
#[cfg(test)]
mod bands_tests;
pub(crate) mod curves;
#[cfg(test)]
mod curves_tests;
pub(crate) mod encode;
pub(crate) mod facts;
#[cfg(test)]
mod facts_tests;
#[cfg(target_os = "macos")]
// Metal through the Objective-C runtime: every message is C.
#[allow(unsafe_code)]
pub(crate) mod mtl;
pub(crate) mod quads;
#[cfg(test)]
mod quads_chrome_tests;
#[cfg(test)]
mod quads_icon_tests;
#[cfg(test)]
mod quads_tests;
pub(crate) mod record;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
pub(crate) mod reference_tests;
pub(crate) mod rows;
#[cfg(test)]
mod rows_plan_tests;
#[cfg(test)]
mod rows_tests;
pub(crate) mod shrink;
#[cfg(test)]
mod shrink_pen_tests;
#[cfg(test)]
mod shrink_tests;
pub(crate) mod textures;
#[cfg(test)]
mod textures_model_tests;
#[cfg(test)]
mod textures_tests;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
// Our Vulkan bindings: every driver call is C.
#[allow(unsafe_code)]
pub(crate) mod vk;
