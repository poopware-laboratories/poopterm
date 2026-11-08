//! The header bar's model: what each visible tab and button shows, in which
//! colors, from the tab layout, the tabs' state and the pointer.
use crate::{
	render::{
		blend::blend,
		header::{Badge, Header, HeaderButton, HeaderColors, HeaderTab, Icon, Rect, close_rect},
		label::Label,
		window_icons::WindowIcon,
	},
	term::{
		status::{Progress, ProgressState},
		tab_style::TabStyle,
	},
	theme::Theme,
	ui::{
		chrome_colors::chrome_colors,
		contrast::tab_text_colors,
		tabs::{Marker, TabLayout},
		window_controls::{Placement, WindowButton},
	},
};

/// What the header needs to know about one tab.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TabInfo {
	pub title: String,
	pub style: TabStyle,
	pub marker: Option<Marker>,
	pub progress: Option<Progress>,
}

pub(crate) struct HeaderInput<'a> {
	pub layout: &'a TabLayout,
	/// The visible tabs, the first at `layout.first_visible`.
	pub tabs: &'a [TabInfo],
	/// How many tabs there are, visible or not.
	pub count: usize,
	pub active: usize,
	pub cursor: (f64, f64),
	/// The header's top and height.
	pub bounds: (usize, usize),
	pub px: f32,
	pub theme: &'a Theme,
}

/// A button's height, before scaling.
const BUTTON_HEIGHT: usize = 32;
/// A divider's height, before scaling.
const DIVIDER_HEIGHT: usize = 20;

/// The header's colors: the theme's chrome colors, with disabled icons in
/// the muted text color.
pub(crate) fn header_colors(theme: &Theme) -> HeaderColors {
	let chrome = chrome_colors(theme);
	HeaderColors {
		bar: chrome.chrome,
		separator: chrome.hairline,
		hover: chrome.hover,
		raised: chrome.raised,
		fg: theme.fg,
		dim: chrome.muted,
	}
}

pub(crate) fn header<'a>(input: &HeaderInput<'a>) -> Header<'a> {
	let (layout, theme) = (input.layout, input.theme);
	let palette = TabPalette::new(theme);
	let colors = palette.colors;
	let scale = layout.scale;
	let tabs = input.tabs.iter().enumerate().map(|(local, tab)| {
		let index = layout.first_visible + local;
		let (x, width) = (layout.tab_x(index), layout.tab_width(index));
		let active = index == input.active;
		let hovered = contains((x, input.bounds.0, width, input.bounds.1), input.cursor);
		let close = (input.count > 1 && (active || hovered))
			.then(|| contains(close_rect(x, width, input.bounds, scale), input.cursor));
		header_tab(tab, &palette, (x, width), (active, hovered), close)
	});
	let height = BUTTON_HEIGHT * scale;
	let top = input.bounds.0 + input.bounds.1.saturating_sub(height) / 2;
	let button = |x, width, icon, enabled| {
		let rect = (x, top, width, height);
		let hovered = contains(rect, input.cursor);
		HeaderButton {
			rect,
			icon,
			hovered,
			enabled,
			pressed: false,
		}
	};
	let mut buttons = Vec::new();
	if layout.arrow_w > 0 {
		let earlier = layout.first_visible > 0;
		let more = layout.first_visible + layout.visible_count < input.count;
		let (left, right) = (layout.left_arrow_x, layout.right_arrow_x);
		buttons.push(button(left, layout.arrow_w, Icon::Left, earlier));
		buttons.push(button(right, layout.arrow_w, Icon::Right, more));
	}
	if layout.plus_w > 0 {
		buttons.push(button(layout.plus_x, layout.plus_w, Icon::Plus, true));
	}
	if layout.settings_w > 0 {
		buttons.push(button(
			layout.settings_x,
			layout.settings_w,
			Icon::Menu,
			true,
		));
	}
	Header {
		y: input.bounds.0,
		height: input.bounds.1,
		scale,
		px: input.px,
		colors,
		tabs: tabs.collect(),
		buttons,
		dividers: Vec::new(),
	}
}

/// What the window buttons show of the window.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowState {
	/// Maximize shows as restore.
	pub maximized: bool,
	/// Without focus the buttons are dim.
	pub focused: bool,
	/// The button held down, pressed while the pointer stays on it.
	pub pressed: Option<WindowButton>,
}

/// Adds the window buttons `controls` places, and their dividers, to
/// `header`, centered in it as [`crate::ui::window_controls::button_at`]
/// finds them.
pub(crate) fn add_window_buttons(
	header: &mut Header,
	controls: &Placement,
	window: WindowState,
	cursor: (f64, f64),
) {
	let (y, height, scale) = (header.y, header.height, header.scale);
	let middle = |size: usize| y + height.saturating_sub(size * scale) / 2;
	for &(kind, x) in &controls.buttons {
		let rect = (
			x,
			middle(BUTTON_HEIGHT),
			controls.width,
			BUTTON_HEIGHT * scale,
		);
		let icon = match kind {
			WindowButton::Minimize => WindowIcon::Minimize,
			WindowButton::Maximize if window.maximized => WindowIcon::Restore,
			WindowButton::Maximize => WindowIcon::Maximize,
			WindowButton::Close => WindowIcon::Close,
		};
		let hovered = contains(rect, cursor);
		header.buttons.push(HeaderButton {
			rect,
			icon: Icon::Window(icon),
			hovered,
			enabled: window.focused,
			pressed: hovered && window.pressed == Some(kind),
		});
	}
	let divider = |&x: &usize| (x, middle(DIVIDER_HEIGHT), scale, DIVIDER_HEIGHT * scale);
	header
		.dividers
		.extend(controls.dividers.iter().map(divider));
}

/// The colors tabs are drawn in, worked out once per frame: finding
/// readable text colors takes a search.
#[derive(Clone, Copy)]
pub(crate) struct TabPalette<'a> {
	pub theme: &'a Theme,
	pub colors: HeaderColors,
	pub dim: u32,
	pub accent: u32,
}

impl<'a> TabPalette<'a> {
	pub(crate) fn new(theme: &'a Theme) -> Self {
		let (dim, accent) = tab_text_colors(theme);
		let colors = header_colors(theme);
		Self {
			theme,
			colors,
			dim,
			accent,
		}
	}
}

/// How `tab` looks at `(x, width)`, `active` or `hovered`, with its close
/// button when `close` is `Some(hovered)`.
pub(crate) fn header_tab<'a>(
	tab: &'a TabInfo,
	palette: &TabPalette,
	(x, width): (usize, usize),
	(active, hovered): (bool, bool),
	close: Option<bool>,
) -> HeaderTab<'a> {
	let TabPalette {
		theme,
		colors,
		dim,
		accent,
	} = *palette;
	let fill = match tab.style.color {
		Some(color) => {
			let shade = if active {
				150
			} else if hovered {
				110
			} else {
				80
			};
			Some(blend(colors.bar, color, shade))
		}
		None if active => Some(colors.raised),
		None if hovered => Some(colors.hover),
		None => None,
	};
	// The terminal's own markers say more than a program's indicator.
	let badge = tab
		.marker
		.map(|marker| match marker {
			Marker::Bell => Badge::Bell(accent),
			Marker::Blocked => Badge::Dot(theme.palette[3]),
			Marker::Error => Badge::Dot(theme.palette[1]),
			Marker::Done => Badge::Dot(theme.palette[2]),
			Marker::Working => Badge::Dot(accent),
			Marker::Activity => Badge::Dot(dim),
		})
		.or(tab.style.indicator.map(Badge::Dot));
	let progress = tab.progress.map(|progress| {
		let color = match progress.state {
			ProgressState::Error => theme.palette[1],
			ProgressState::Paused => theme.palette[3],
			_ => theme.palette[4],
		};
		(progress.percent, color)
	});
	// A tint is drawn for the title in full; plain tabs at rest stay quiet.
	let quiet = !active && !hovered && tab.style.color.is_none();
	let title = if quiet { colors.dim } else { theme.fg };
	HeaderTab {
		x,
		width,
		label: Label {
			title: &tab.title,
			status: tab.style.status.as_deref(),
			colors: (title, tab.style.status_color.unwrap_or(dim)),
		},
		fill,
		badge,
		progress,
		close,
	}
}

/// Whether `rect` holds the point `(x, y)`.
pub(crate) fn contains((left, top, width, height): Rect, (x, y): (f64, f64)) -> bool {
	x >= left as f64 && x < (left + width) as f64 && y >= top as f64 && y < (top + height) as f64
}
