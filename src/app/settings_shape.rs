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
	/// physical pixels at the top and bottom kept for the system.
	Top { inset: (usize, usize) },
}

/// The shape of a window `width` physical pixels wide at `scale`, where
/// the system keeps `inset`: the list goes on top when the smallest page
/// would not fit beside it.
pub(super) fn shape(width: usize, scale: usize, inset: (usize, usize)) -> Shape {
	if width / scale.max(1) >= MIN_SIZE.0 + NAV_WIDTH {
		Shape::Side
	} else {
		Shape::Top { inset }
	}
}

impl Shape {
	/// The page's rectangle in a window `(width, height)` large.
	pub(super) fn page(self, (width, height): (usize, usize), s: usize) -> Rect {
		match self {
			Shape::Side => {
				let nav = NAV_WIDTH * s;
				(nav, 0, width.saturating_sub(nav), height)
			}
			Shape::Top {
				inset: (top, bottom),
			} => {
				let y = (top + HEAD * s).min(height);
				(0, y, width, height.saturating_sub(y + bottom))
			}
		}
	}

	/// Item `index`'s rectangle in a window `width` wide; across the top,
	/// the columns share the width.
	pub(super) fn item(self, index: usize, width: usize, s: usize) -> Rect {
		let Shape::Top { inset: (top, _) } = self else {
			return item_rect(index, s);
		};
		let (column, row) = (index % COLUMNS, index / COLUMNS);
		let room = width.saturating_sub((2 * SIDE + (COLUMNS - 1) * GAP) * s);
		let w = room / COLUMNS;
		let x = SIDE * s + column * (w + GAP * s);
		(x, top + (BAR + row * STRIDE) * s, w, ITEM_HEIGHT * s)
	}

	/// The Done button in a window `width` wide; none beside the list,
	/// where the window has its own way to close.
	pub(super) fn done(self, width: usize, s: usize) -> Option<Rect> {
		let Shape::Top { inset: (top, _) } = self else {
			return None;
		};
		let x = width.saturating_sub((DONE + 2 * SIDE) * s);
		Some((x, top + DONE_TOP * s, DONE * s, ITEM_HEIGHT * s))
	}

	/// What a click at physical `point` does in a window `size` large with
	/// `page` open: the list opens pages, Done closes, and the page sees
	/// the click as if it stood alone.
	pub(super) fn target(
		self,
		point: (usize, usize),
		size: (usize, usize),
		s: usize,
		page: SettingsPage,
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
		if !(py..py + ph).contains(&y) {
			return None;
		}
		settings_target((x - px, y - py), (pw, ph), s, page)
	}
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
		pen.tinted(p, (SIDE + 8) * s, done.1 / s + 6, "Settings", 0.9, fg);
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
