//! Pixel-level drawing helpers shared by the terminal and settings windows.
pub(crate) mod backend;
pub(crate) mod background;
pub(crate) mod blend;
pub(crate) mod boxdraw;
pub(crate) mod cell_joins;
pub(crate) mod chrome;
#[cfg(test)]
mod chrome_scene_tests;
pub(crate) mod damage;
#[cfg(test)]
mod dim_tests;
pub(crate) mod fit;
pub(crate) mod fxhash;
pub(crate) mod glyphs;
pub(crate) mod grid;
#[cfg(test)]
pub(crate) mod grid_case_tests;
pub(crate) mod header;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod header_window_tests;
pub(crate) mod images;
pub(crate) mod label;
#[cfg(test)]
mod label_tests;
mod multicell;
pub(crate) mod padding;
pub(crate) mod painter;
mod pane_view;
#[cfg(test)]
mod pane_view_tests;
pub(crate) mod panes;
#[cfg(test)]
mod panes_tests;
pub(crate) mod pen;
#[cfg(test)]
mod pen_tests;
#[cfg(test)]
mod pen_untracked_tests;
pub(crate) mod popover;
#[cfg(test)]
mod popover_tests;
#[cfg(test)]
mod radius_scene_tests;
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
pub(crate) mod window_icons;
#[cfg(test)]
mod window_icons_tests;

#[cfg(test)]
mod backend_tests;
#[cfg(test)]
mod emoji_tests;
#[cfg(test)]
mod presentation_tests;
