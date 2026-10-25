//! The header bar's model: what each visible tab and button shows, in which
//! colors, from the tab layout, the tabs' state and the pointer.
use crate::{
	render::{
		blend::blend,
		header::{Badge, Header, HeaderButton, HeaderColors, HeaderTab, Icon, Rect, close_rect},
		label::Label,
	},
	term::{
		status::{Progress, ProgressState},
		tab_style::TabStyle,
	},
	theme::Theme,
	ui::{
		contrast::tab_text_colors,
		tabs::{Marker, TabLayout},
	},
};

/// What the header needs to know about one tab.
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

/// How far each header shade moves from the background toward the text,
/// out of 255.
const BAR: u32 = 10;
const HOVER: u32 = 22;
const RAISED: u32 = 34;
const SEPARATOR: u32 = 60;
/// Disabled icons, from the bar toward the text.
const DISABLED: u32 = 128;
/// A button's height, before scaling.
const BUTTON_HEIGHT: usize = 32;

pub(crate) fn header_colors(theme: &Theme) -> HeaderColors {
	let shade = |amount| blend(theme.bg, theme.fg, amount);
	HeaderColors {
		bar: shade(BAR),
		separator: shade(SEPARATOR),
		hover: shade(HOVER),
		raised: shade(RAISED),
		fg: theme.fg,
		dim: blend(shade(BAR), theme.fg, DISABLED),
	}
}

pub(crate) fn header<'a>(input: &HeaderInput<'a>) -> Header<'a> {
	let (layout, theme) = (input.layout, input.theme);
	let colors = header_colors(theme);
	let (dim, accent) = tab_text_colors(theme);
	let scale = layout.scale;
	let tabs = input.tabs.iter().enumerate().map(|(local, tab)| {
		let index = layout.first_visible + local;
		let (x, width) = (layout.tab_x(index), layout.tab_width(index));
		let active = index == input.active;
		let hovered = contains((x, input.bounds.0, width, input.bounds.1), input.cursor);
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
		let close = (input.count > 1 && (active || hovered))
			.then(|| contains(close_rect(x, width, input.bounds, scale), input.cursor));
		HeaderTab {
			x,
			width,
			label: Label {
				title: &tab.title,
				status: tab.style.status.as_deref(),
				colors: (theme.fg, tab.style.status_color.unwrap_or(dim)),
			},
			fill,
			badge,
			progress,
			close,
		}
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
	}
}

/// Whether `rect` holds the point `(x, y)`.
fn contains((left, top, width, height): Rect, (x, y): (f64, f64)) -> bool {
	x >= left as f64 && x < (left + width) as f64 && y >= top as f64 && y < (top + height) as f64
}
