//! Data for the GPU renderers, kept free of GPU calls so it is tested like
//! any other code: glyph outlines laid out for coverage shaders and stored
//! in a curve buffer, the atlas that holds bitmaps, and the grid as quads;
//! and our own Vulkan bindings.
pub(crate) mod atlas;
#[cfg(test)]
mod atlas_growth_tests;
#[cfg(test)]
mod atlas_tests;
pub(crate) mod bands;
#[cfg(test)]
mod bands_tests;
pub(crate) mod curves;
#[cfg(test)]
mod curves_tests;
pub(crate) mod quads;
#[cfg(test)]
mod quads_chrome_tests;
#[cfg(test)]
mod quads_tests;
pub(crate) mod record;
#[cfg(test)]
mod record_tests;
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
pub(crate) mod vk;
