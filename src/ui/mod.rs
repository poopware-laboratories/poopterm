//! Window-system-independent UI models: selection, search and tab layout.
pub(crate) mod about;
#[cfg(test)]
mod about_tests;
pub(crate) mod chrome_colors;
#[cfg(test)]
mod chrome_colors_tests;
pub(crate) mod contrast;
pub(crate) mod copy;
#[cfg_attr(
	target_os = "macos",
	allow(dead_code, reason = "macOS keeps its own title bar")
)]
pub(crate) mod decorations;
#[cfg(test)]
mod decorations_tests;
pub(crate) mod diagnostics;
pub(crate) mod diagnostics_colors;
#[cfg(test)]
mod diagnostics_colors_tests;
pub(crate) mod diagnostics_page;
#[cfg(test)]
mod diagnostics_page_property_tests;
#[cfg(test)]
mod diagnostics_page_tests;
#[cfg(test)]
mod diagnostics_property_tests;
#[cfg(test)]
mod diagnostics_tests;
pub(crate) mod font_size;
#[cfg(test)]
mod font_size_tests;
pub(crate) mod header;
pub(crate) mod header_target;
#[cfg(test)]
mod header_target_property_tests;
#[cfg(test)]
mod header_target_tests;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod header_window_tests;
pub(crate) mod layout;
pub(crate) mod links;
pub(crate) mod metrics;
#[cfg(test)]
mod metrics_tests;
pub(crate) mod overlays;
#[cfg(test)]
mod overlays_tests;
pub(crate) mod pane_sessions;
#[cfg(test)]
mod pane_sessions_tests;
pub(crate) mod panes;
#[cfg(test)]
mod panes_table_tests;
#[cfg(test)]
mod panes_tests;
pub(crate) mod scale;
pub(crate) mod search;
pub(crate) mod search_controls;
#[cfg(test)]
mod search_controls_tests;
pub(crate) mod selection;
#[cfg(test)]
mod selection_tests;
pub(crate) mod settings_layout;
#[cfg(test)]
mod settings_layout_tests;
pub(crate) mod sidebar;
#[cfg(test)]
mod sidebar_tests;
pub(crate) mod status_bar;
#[cfg(test)]
mod status_bar_tests;
pub(crate) mod tabs;
#[cfg(test)]
mod tabs_arrows_tests;
#[cfg(test)]
mod tabs_horizontal_edge_tests;
#[cfg(test)]
mod tabs_horizontal_tests;
#[cfg(test)]
mod tabs_vertical_edge_tests;
#[cfg(test)]
mod tabs_vertical_tests;
#[cfg(test)]
mod tabs_window_reserve_tests;
pub(crate) mod ui_font;
#[cfg(test)]
mod ui_font_drawn_tests;
#[cfg(test)]
mod ui_font_tests;
#[cfg_attr(
	target_os = "macos",
	allow(dead_code, reason = "macOS keeps its own window buttons")
)]
pub(crate) mod window_controls;
#[cfg(test)]
mod window_controls_property_tests;
#[cfg(test)]
mod window_controls_tests;
pub(crate) mod window_pointer;
#[cfg(test)]
mod window_pointer_property_tests;
#[cfg(test)]
mod window_pointer_tests;
pub(crate) mod zoom;
