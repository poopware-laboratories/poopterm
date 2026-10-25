//! Data for the GPU renderers, kept free of GPU calls so it is tested like
//! any other code: glyph outlines laid out for coverage shaders and stored
//! in a curve buffer, and the atlas that holds color bitmaps.
pub(crate) mod atlas;
#[cfg(test)]
mod atlas_tests;
pub(crate) mod bands;
#[cfg(test)]
mod bands_tests;
pub(crate) mod curves;
#[cfg(test)]
mod curves_tests;
pub(crate) mod record;
#[cfg(test)]
mod record_tests;
