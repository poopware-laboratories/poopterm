//! Window-system-independent UI models: selection, search and tab layout.
pub(crate) mod copy;
pub(crate) mod layout;
pub(crate) mod links;
pub(crate) mod scale;
pub(crate) mod search;
pub(crate) mod selection;
#[cfg(test)]
mod selection_tests;
pub(crate) mod tabs;
#[cfg(test)]
mod tabs_horizontal_edge_tests;
#[cfg(test)]
mod tabs_horizontal_tests;
#[cfg(test)]
mod tabs_vertical_edge_tests;
#[cfg(test)]
mod tabs_vertical_tests;
pub(crate) mod zoom;
