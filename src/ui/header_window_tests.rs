//! Tests of the window buttons in the header model: which icons show, in
//! what state, where, and that they sit exactly where presses hit them.
use super::{
	header::{WindowState, add_window_buttons},
	window_controls::{ButtonLayout, Placement, WindowButton, button_at, place},
};
use crate::render::{
	header::{Header, HeaderButton, HeaderColors, Icon},
	window_icons::WindowIcon,
};

const AWAY: (f64, f64) = (-1.0, -1.0);
const ACTIVE: WindowState = WindowState {
	maximized: false,
	focused: true,
};

fn bare(scale: usize) -> Header<'static> {
	Header {
		y: 0,
		height: 44 * scale,
		scale,
		px: 13.0,
		colors: HeaderColors {
			bar: 0,
			separator: 0,
			hover: 0,
			raised: 0,
			fg: 0,
			dim: 0,
		},
		tabs: Vec::new(),
		buttons: Vec::new(),
		dividers: Vec::new(),
	}
}

fn with(layout: &str, window: WindowState, cursor: (f64, f64)) -> (Header<'static>, Placement) {
	let placement = place(&ButtonLayout::parse(layout), 1100, 1);
	let mut header = bare(1);
	add_window_buttons(&mut header, &placement, window, cursor);
	(header, placement)
}

fn icons(header: &Header) -> Vec<Icon> {
	header.buttons.iter().map(|button| button.icon).collect()
}

#[test]
fn no_window_buttons_add_nothing() {
	let (header, _) = with("appmenu:", ACTIVE, AWAY);
	assert!(header.buttons.is_empty() && header.dividers.is_empty());
}

#[test]
fn three_buttons_show_their_icons_in_order_where_they_were_placed() {
	let (header, _) = with("appmenu:minimize,maximize,close", ACTIVE, AWAY);
	use WindowIcon::*;
	assert_eq!(
		icons(&header),
		[Minimize, Maximize, Close].map(Icon::Window)
	);
	let rects: Vec<_> = header.buttons.iter().map(|button| button.rect).collect();
	assert_eq!(
		rects,
		[(984, 6, 36, 32), (1020, 6, 36, 32), (1056, 6, 36, 32)]
	);
	assert!(header.buttons.iter().all(|b| b.enabled && !b.hovered));
}

#[test]
fn a_maximized_window_shows_restore_in_place_of_maximize() {
	let maximized = WindowState {
		maximized: true,
		..ACTIVE
	};
	let (header, _) = with("close,maximize:", maximized, AWAY);
	use WindowIcon::*;
	assert_eq!(icons(&header), [Close, Restore].map(Icon::Window));
}

#[test]
fn an_unfocused_window_disables_every_button() {
	let unfocused = WindowState {
		focused: false,
		..ACTIVE
	};
	let (header, _) = with("minimize:close", unfocused, (1074.0, 22.0));
	assert_eq!(header.buttons.len(), 2);
	assert!(header.buttons.iter().all(|b| !b.enabled));
}

#[test]
fn only_the_button_under_the_pointer_is_hovered() {
	let (header, _) = with("appmenu:minimize,maximize,close", ACTIVE, (1074.0, 22.0));
	let hovered: Vec<_> = header.buttons.iter().map(|b| b.hovered).collect();
	assert_eq!(hovered, [false, false, true]);
	// The divider gap between buttons and tabs is not a button.
	let (header, _) = with("appmenu:minimize,maximize,close", ACTIVE, (975.0, 22.0));
	assert!(header.buttons.iter().all(|b| !b.hovered));
}

#[test]
fn dividers_are_one_pixel_by_twenty_centered_in_the_bar() {
	let (header, _) = with("close:maximize", ACTIVE, AWAY);
	assert_eq!(header.dividers, [(52, 12, 1, 20), (1047, 12, 1, 20)]);
	let placement = place(&ButtonLayout::fallback(), 2200, 2);
	let mut big = bare(2);
	add_window_buttons(&mut big, &placement, ACTIVE, AWAY);
	assert_eq!(big.dividers, [(1950, 24, 2, 40)]);
	assert_eq!(big.buttons[0].rect, (1968, 12, 72, 64));
}

#[test]
fn buttons_follow_the_header_down_the_window() {
	let placement = place(&ButtonLayout::fallback(), 1100, 1);
	let mut lower = bare(1);
	lower.y = 30;
	add_window_buttons(&mut lower, &placement, ACTIVE, AWAY);
	assert_eq!(lower.buttons[0].rect.1, 36);
	assert_eq!(lower.dividers[0].1, 42);
}

#[test]
fn window_buttons_come_after_the_header_buttons_already_there() {
	let placement = place(&ButtonLayout::fallback(), 1100, 1);
	let mut header = bare(1);
	header.buttons.push(HeaderButton {
		rect: (891, 6, 36, 32),
		icon: Icon::Plus,
		hovered: false,
		enabled: true,
	});
	add_window_buttons(&mut header, &placement, ACTIVE, AWAY);
	assert_eq!(header.buttons[0].icon, Icon::Plus);
	assert_eq!(header.buttons.len(), 4);
}

#[test]
fn every_drawn_button_is_where_a_press_hits_it() {
	let layouts = [
		"appmenu:minimize,maximize,close",
		"close,minimize,maximize:",
		"close:maximize",
		"minimize:close",
	];
	for scale in 1..=3 {
		for text in layouts {
			let placement = place(&ButtonLayout::parse(text), 1100 * scale, scale);
			let mut header = bare(scale);
			add_window_buttons(&mut header, &placement, ACTIVE, AWAY);
			let bar = (header.y, header.height);
			for (button, &(kind, _)) in header.buttons.iter().zip(&placement.buttons) {
				let (x, y, w, h) = button.rect;
				let kind_of = |icon| match icon {
					Icon::Window(WindowIcon::Minimize) => Some(WindowButton::Minimize),
					Icon::Window(WindowIcon::Maximize | WindowIcon::Restore) => Some(WindowButton::Maximize),
					Icon::Window(WindowIcon::Close) => Some(WindowButton::Close),
					_ => None,
				};
				assert_eq!(kind_of(button.icon), Some(kind), "{text}");
				for point in [(x, y), (x + w - 1, y + h - 1), (x + w / 2, y + h / 2)] {
					let point = (point.0 as f64, point.1 as f64);
					assert_eq!(
						button_at(&placement, point, bar, scale),
						Some(kind),
						"{text}"
					);
				}
				for point in [(x, y + h), (x + w / 2, y - 1)] {
					let point = (point.0 as f64, point.1 as f64);
					assert_eq!(button_at(&placement, point, bar, scale), None, "{text}");
				}
			}
		}
	}
}
