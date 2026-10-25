//! Data for the GPU renderers, kept free of GPU calls so it is tested like
//! any other code: glyph outlines laid out for coverage shaders, and the
//! atlas that holds color bitmaps.
pub(crate) mod atlas;
#[cfg(test)]
mod atlas_tests;
pub(crate) mod bands;
#[cfg(test)]
mod bands_tests;
