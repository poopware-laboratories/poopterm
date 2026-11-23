//! How the Settings window is shared between its list of pages and the open
//! page: the list down the left of a window wide enough for both, or, on a
//! phone's narrow screen, across the top under the status bar, beside a
//! Done button, with the page under it kept clear of the home indicator.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_nav::{ITEMS, NAV_WIDTH, item_rect, window_target},
	settings_pen::Pen,
	settings_view::MIN_SIZE,
};

/// The height a page is laid out for, in logical pixels: its rows and its
/// footer fit in this much. A shorter page across the top scrolls.
pub(super) const PAGE_HEIGHT: usize = MIN_SIZE.1;
use crate::{
	render::{
		blend::blend,
		painter::ButtonStyle,
		scene::{Rect, Scene},
	},
	ui::{chrome_colors::ChromeColors, metrics::Corner},
};

/// The bar with the title and Done, in logical pixels.
pub(super) const BAR: usize = 44;
/// Pages per row of the list across the top.
const COLUMNS: usize = 4;
const ROWS: usize = ITEMS.len().div_ceil(COLUMNS);
/// An item's height, the distance from one row's top to the next's, and
/// the room around and between items, in logical pixels.
const ITEM_HEIGHT: usize = 32;
const STRIDE: usize = 36;
const SIDE: usize = 8;
const GAP: usize = 4;
/// The Done button's width and its distance from the bar's top.
const DONE: usize = 72;
const DONE_TOP: usize = 6;
/// The bar and the list under it, in logical pixels.
pub(super) const HEAD: usize = BAR + ROWS * STRIDE + SIDE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
	/// The list at the left, the page beside it.
	Side,
	/// The list across the top and the page under it, with `inset`
	/// physical pixels at the top and bottom, and `sides` at the left and
	/// right, kept for the system.
	Top {
		inset: (usize, usize),
		sides: (usize, usize),
	},
}

/// The shape of a window `width` physical pixels wide at `scale`, where
/// the system keeps `inset`: the list goes on top when the smallest page
/// would not fit beside it, or when the system gives the window no close
/// button of its own (`closable` false, as on iOS), since only the list on
/// top offers Done.
pub(super) fn shape(width: usize, scale: usize, inset: (usize, usize), closable: bool) -> Shape {
	if closable && width / scale.max(1) >= MIN_SIZE.0 + NAV_WIDTH {
		Shape::Side
	} else {
		Shape::Top {
			inset,
			sides: (0, 0),
		}
	}
}

impl Shape {
	/// This shape with `sides` pixels at the left and right kept for the
	/// system (a phone's sensor housing, an iPhone Duo's clock); the
	/// sidebar's window has its own frame and keeps none.
	pub(super) fn beside(self, sides: (usize, usize)) -> Self {
		match self {
			Shape::Side => Shape::Side,
			Shape::Top { inset, .. } => Shape::Top { inset, sides },
		}
	}

	/// The page's rectangle in a window `(width, height)` large.
	pub(super) fn page(self, (width, height): (usize, usize), s: usize) -> Rect {
		match self {
			Shape::Side => {
				let nav = NAV_WIDTH * s;
				(nav, 0, width.saturating_sub(nav), height)
			}
			Shape::Top {
				inset: (top, bottom),
				sides: (left, right),
			} => {
				let y = (top + HEAD * s).min(height);
				let x = left.min(width);
				let w = width.saturating_sub(x + right);
				(x, y, w, height.saturating_sub(y + bottom))
			}
		}
	}

	/// Item `index`'s rectangle in a window `width` wide; across the top,
	/// the columns share the width.
	pub(super) fn item(self, index: usize, width: usize, s: usize) -> Rect {
		let Shape::Top {
			inset: (top, _),
			sides: (left, right),
		} = self
		else {
			return item_rect(index, s);
		};
		let (column, row) = (index % COLUMNS, index / COLUMNS);
		let room = width.saturating_sub(left + right + (2 * SIDE + (COLUMNS - 1) * GAP) * s);
		let w = room / COLUMNS;
		let x = left + SIDE * s + column * (w + GAP * s);
		(x, top + (BAR + row * STRIDE) * s, w, ITEM_HEIGHT * s)
	}

	/// The Done button in a window `width` wide; none beside the list,
	/// where the window has its own way to close.
	pub(super) fn done(self, width: usize, s: usize) -> Option<Rect> {
		let Shape::Top {
			inset: (top, _),
			sides: (_, right),
		} = self
		else {
			return None;
		};
		let x = width.saturating_sub(right + (DONE + 2 * SIDE) * s);
		Some((x, top + DONE_TOP * s, DONE * s, ITEM_HEIGHT * s))
	}

	/// How tall the page is drawn, when `visible` pixels of it show: as
	/// tall as it is laid out for across the top, where a short window
	/// (a phone on its side) scrolls it; beside the list, the window is
	/// never shorter than that.
	pub(super) fn extent(self, visible: usize, s: usize) -> usize {
		match self {
			Shape::Side => visible,
			Shape::Top { .. } => visible.max(PAGE_HEIGHT * s),
		}
	}

	/// What a click at physical `point` does in a window `size` large with
	/// `page` open and scrolled down by `scroll` pixels: the list opens
	/// pages, Done closes, and the page sees the click as if it stood
	/// alone, unscrolled.
	pub(super) fn target(
		self,
		point: (usize, usize),
		size: (usize, usize),
		s: usize,
		(page, scroll): (SettingsPage, usize),
	) -> Option<SettingsTarget> {
		if self == Shape::Side {
			return window_target(point, size, s, page);
		}
		let (x, y) = point;
		let held = |(rx, ry, rw, rh): Rect| (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y);
		if self.done(size.0, s).is_some_and(held) {
			return Some(SettingsTarget::Done);
		}
		if let Some(index) = (0..ITEMS.len()).find(|&index| held(self.item(index, size.0, s))) {
			return Some(SettingsTarget::Open(ITEMS[index].0));
		}
		let (px, py, pw, ph) = self.page(size, s);
		if !(py..py + ph).contains(&y) || !(px..px + pw).contains(&x) {
			return None;
		}
		let extent = self.extent(ph, s);
		let scroll = clamp_scroll(scroll, ph, extent);
		settings_target((x - px, y - py + scroll), (pw, extent), s, page)
	}
}

/// `scroll` kept between the top and where the page's end, `extent` tall,
/// meets the bottom of the `visible` part.
pub(super) fn clamp_scroll(scroll: usize, visible: usize, extent: usize) -> usize {
	scroll.min(extent.saturating_sub(visible))
}

/// `scroll` moved by `pixels` (positive scrolls further down the page),
/// kept inside the page.
pub(super) fn scrolled(scroll: usize, pixels: f64, visible: usize, extent: usize) -> usize {
	let moved = scroll as f64 + if pixels.is_finite() { pixels } else { 0.0 };
	clamp_scroll(moved.max(0.0).round() as usize, visible, extent)
}

/// Draws the list across the top of a window: its panel down to the page
/// with a line under it, the title and Done, then an item for each page,
/// the open `page` filled and the one under `cursor` lit.
pub(super) fn compose_top(
	pen: &mut Pen,
	p: &mut Scene,
	shape: Shape,
	(page, cursor): (SettingsPage, (f64, f64)),
	(colors, fg, style): (&ChromeColors, u32, ButtonStyle),
) {
	let (s, width) = (pen.scale, p.width);
	let bottom = shape.page((width, p.height), s).1;
	p.rect(0, 0, width, bottom, colors.chrome);
	p.rect(0, bottom.saturating_sub(s), width, s, colors.hairline);
	let under = |(x, y, w, h): Rect| {
		cursor.0 >= x as f64
			&& cursor.0 < (x + w) as f64
			&& cursor.1 >= y as f64
			&& cursor.1 < (y + h) as f64
	};
	if let Some(done) = shape.done(width, s) {
		let left = match shape {
			Shape::Top { sides, .. } => sides.0,
			Shape::Side => 0,
		};
		pen.tinted(
			p,
			left + (SIDE + 8) * s,
			done.1 / s + 6,
			"Settings",
			0.9,
			fg,
		);
		p.focusable(done);
		p.button(done, style, under(done));
		pen.centered(p, done, "Done", 0.82);
	}
	let radius = Corner::Control.radius(s);
	for (index, &(item, label)) in ITEMS.iter().enumerate() {
		let rect = shape.item(index, width, s);
		p.focusable(rect);
		let open = item == page;
		let (x, y, w, h) = rect;
		if open {
			p.round_rect(x, y, w, h, radius, blend(colors.chrome, fg, 36));
		} else if under(rect) {
			p.round_rect(x, y, w, h, radius, colors.hover);
		}
		let color = if open { fg } else { colors.muted };
		let label = pen.fit(label, w.saturating_sub(8 * s), 0.72);
		let text_x = x + w.saturating_sub(pen.width(&label, 0.72)) / 2;
		pen.tinted(p, text_x, y / s + 8, &label, 0.72, color);
	}
}
