//! The vertical tab sidebar's model: what the pointer is over, and what each
//! visible tab and button shows, from the layout and the tabs' state.
use crate::{
	render::{
		header::{HeaderButton, Icon, Rect, Style, close_rect},
		sidebar::{Sidebar, SidebarRow, rail_close_rect},
	},
	theme::Theme,
	ui::{
		header::{TabInfo, TabPalette, contains, header_tab},
		tabs::VerticalTabLayout,
	},
};

/// What the pointer is over in the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarTarget {
	Tab { index: usize, close: bool },
	Button(Icon),
}

/// Everything the sidebar shows. Partial redraws repaint the sidebar only
/// when this differs from what is on screen; layout changes (resize, scale,
/// theme, preferences) invalidate the canvas and force a full frame anyway.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SidebarState {
	/// The visible tabs, the first at `first_visible`.
	pub tabs: Vec<TabInfo>,
	pub first_visible: usize,
	/// How many tabs there are, visible or not.
	pub count: usize,
	pub active: usize,
	pub target: Option<SidebarTarget>,
}

pub(crate) struct SidebarInput<'a> {
	pub layout: &'a VerticalTabLayout,
	pub state: &'a SidebarState,
	/// The sidebar's top and height.
	pub bounds: (usize, usize),
	pub px: f32,
	pub theme: &'a Theme,
}

/// The surface of the tab at `index`.
pub(crate) fn row_rect(layout: &VerticalTabLayout, index: usize) -> Rect {
	let scale = layout.scale;
	let height = layout.row_height_for(index).saturating_sub(4 * scale);
	(6 * scale, layout.row_y(index), layout.tab_width, height)
}

/// The close button of the tab whose surface is `row`.
fn row_close_rect(layout: &VerticalTabLayout, (x, y, width, height): Rect) -> Rect {
	if layout.compact_rail {
		rail_close_rect((x, y, width, height), layout.scale)
	} else {
		close_rect(x, width, (y, height), layout.scale)
	}
}

/// The buttons the layout shows, where they are.
fn buttons(layout: &VerticalTabLayout) -> impl Iterator<Item = (Rect, Icon)> + '_ {
	let size = (layout.button_width, layout.button_height);
	[
		(layout.plus_x, layout.plus_y, Icon::Plus),
		(layout.settings_x, layout.settings_y, Icon::Menu),
	]
	.into_iter()
	.filter_map(move |(x, y, icon)| y.map(|y| ((x, y, size.0, size.1), icon)))
}

/// What `cursor` is over, with `count` tabs; a tab's close button counts
/// only while there is another tab to keep.
pub(crate) fn sidebar_target(
	layout: &VerticalTabLayout,
	count: usize,
	cursor: (f64, f64),
) -> Option<SidebarTarget> {
	if let Some((_, icon)) = buttons(layout).find(|&(rect, _)| contains(rect, cursor)) {
		return Some(SidebarTarget::Button(icon));
	}
	let shown = layout.first_visible..count.min(layout.first_visible + layout.visible_count);
	let (index, row) = shown
		.map(|index| (index, row_rect(layout, index)))
		.find(|&(_, row)| contains(row, cursor))?;
	let close = count > 1 && contains(row_close_rect(layout, row), cursor);
	Some(SidebarTarget::Tab { index, close })
}

pub(crate) fn sidebar<'a>(input: &SidebarInput<'a>) -> Sidebar<'a> {
	let (layout, state) = (input.layout, input.state);
	let palette = TabPalette::new(input.theme);
	let rows = state.tabs.iter().enumerate().map(|(local, tab)| {
		let index = state.first_visible + local;
		let rect = row_rect(layout, index);
		let active = index == state.active;
		let hovered =
			matches!(state.target, Some(SidebarTarget::Tab { index: over, .. }) if over == index);
		// The rail has no room to keep the active tab's close button.
		let shown = state.count > 1 && (hovered || active && !layout.compact_rail);
		let close = shown.then_some(state.target == Some(SidebarTarget::Tab { index, close: true }));
		SidebarRow {
			number: index + 1,
			rect,
			tab: header_tab(tab, &palette, (rect.0, rect.2), (active, hovered), close),
		}
	});
	let buttons = buttons(layout).map(|(rect, icon)| HeaderButton {
		rect,
		icon,
		hovered: state.target == Some(SidebarTarget::Button(icon)),
		enabled: true,
	});
	Sidebar {
		area: (0, input.bounds.0, layout.sidebar_width, input.bounds.1),
		style: Style {
			scale: layout.scale,
			px: input.px,
			colors: palette.colors,
		},
		rail: layout.compact_rail,
		rows: rows.collect(),
		buttons: buttons.collect(),
	}
}
