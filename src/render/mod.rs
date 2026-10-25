//! Pixel-level drawing helpers shared by the terminal and settings windows.
pub(crate) mod backend;
pub(crate) mod blend;
pub(crate) mod boxdraw;
pub(crate) mod cell_joins;
pub(crate) mod damage;
pub(crate) mod fit;
pub(crate) mod fxhash;
pub(crate) mod glyphs;
pub(crate) mod grid;
pub(crate) mod images;
pub(crate) mod label;
#[cfg(test)]
mod label_tests;
mod multicell;
pub(crate) mod padding;
pub(crate) mod painter;

#[cfg(test)]
mod backend_tests;
#[cfg(test)]
mod emoji_tests;
#[cfg(test)]
mod presentation_tests;
