//! Pixel-level drawing helpers shared by the terminal and settings windows.
pub(crate) mod backend;
pub(crate) mod blend;
pub(crate) mod boxdraw;
pub(crate) mod cell_joins;
#[cfg(test)]
mod chrome_scene_tests;
pub(crate) mod damage;
pub(crate) mod fit;
pub(crate) mod fxhash;
pub(crate) mod glyphs;
pub(crate) mod grid;
pub(crate) mod header;
#[cfg(test)]
mod header_tests;
pub(crate) mod images;
pub(crate) mod label;
#[cfg(test)]
mod label_tests;
mod multicell;
pub(crate) mod padding;
pub(crate) mod painter;
pub(crate) mod popover;
#[cfg(test)]
mod popover_tests;
pub(crate) mod scene;
#[cfg(test)]
mod scene_tests;
pub(crate) mod shapes;
#[cfg(test)]
mod shapes_tests;
pub(crate) mod sidebar;
#[cfg(test)]
mod sidebar_tests;
pub(crate) mod status_bar;
#[cfg(test)]
mod status_bar_tests;

#[cfg(test)]
mod backend_tests;
#[cfg(test)]
mod emoji_tests;
#[cfg(test)]
mod presentation_tests;
