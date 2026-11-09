//! The search box's controls: where its field and its four buttons sit,
//! which one a point is on, and what each shows.
use super::overlays::{FIELD_HEIGHT, INSET, MARGIN, Place};
use crate::render::header::Rect;

/// A button's side, before scaling.
pub(crate) const BUTTON: usize = 28;
const BUTTON_GAP: usize = 2;
/// The room between the field and the first button.
const FIELD_GAP: usize = 4;
const PANEL_WIDTH: usize = 380;
/// The narrowest a field may get before the buttons are left out.
const MIN_FIELD: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchControl {
	Previous,
	Next,
	Case,
	Close,
}

impl SearchControl {
	pub(crate) const ALL: [Self; 4] = [Self::Previous, Self::Next, Self::Case, Self::Close];

	pub(crate) fn glyph(self) -> &'static str {
		match self {
			Self::Previous => "↑",
			Self::Next => "↓",
			Self::Case => "Aa",
			Self::Close => "×",
		}
	}
}

/// Where the search box's parts sit, in physical pixels.
pub(crate) struct Geometry {
	pub panel: Rect,
	pub field: Rect,
	pub buttons: Vec<(SearchControl, Rect)>,
}

/// The width the four buttons and the gaps between them take.
fn controls_width(scale: usize) -> usize {
	let count = SearchControl::ALL.len();
	(count * BUTTON + (count - 1) * BUTTON_GAP) * scale
}

/// Whether a panel `panel_width` wide has room for the buttons beside a
/// field of at least [`MIN_FIELD`].
pub(crate) fn needs_buttons(panel_width: usize, scale: usize) -> bool {
	let room = panel_width.saturating_sub(2 * INSET * scale);
	room >= controls_width(scale) + (FIELD_GAP + MIN_FIELD) * scale
}

pub(crate) fn geometry(place: Place) -> Geometry {
	let Place { window, top, scale } = place;
	let width = (PANEL_WIDTH * scale).min(window.0.saturating_sub(2 * MARGIN * scale));
	let height = (FIELD_HEIGHT + 2 * INSET) * scale;
	let x = window.0.saturating_sub(width + MARGIN * scale);
	let y = top + INSET * scale;
	let panel = (x, y, width, height);
	let full = (
		x + INSET * scale,
		y + INSET * scale,
		width.saturating_sub(2 * INSET * scale),
		FIELD_HEIGHT * scale,
	);
	if !needs_buttons(width, scale) {
		return Geometry {
			panel,
			field: full,
			buttons: Vec::new(),
		};
	}
	let field = (
		full.0,
		full.1,
		full.2 - controls_width(scale) - FIELD_GAP * scale,
		full.3,
	);
	let first = field.0 + field.2 + FIELD_GAP * scale;
	let top = field.1 + (FIELD_HEIGHT - BUTTON) / 2 * scale;
	let buttons = SearchControl::ALL
		.into_iter()
		.enumerate()
		.map(|(index, control)| {
			let left = first + index * (BUTTON + BUTTON_GAP) * scale;
			(control, (left, top, BUTTON * scale, BUTTON * scale))
		})
		.collect();
	Geometry {
		panel,
		field,
		buttons,
	}
}

fn contains((x, y, w, h): Rect, (px, py): (f64, f64)) -> bool {
	px >= x as f64 && px < (x + w) as f64 && py >= y as f64 && py < (y + h) as f64
}

/// The button under `point`, if any.
pub(crate) fn control_at(place: Place, point: (f64, f64)) -> Option<SearchControl> {
	let found = geometry(place).buttons;
	found
		.into_iter()
		.find(|&(_, rect)| contains(rect, point))
		.map(|(control, _)| control)
}

/// Whether `point` is on the search box, buttons or not.
pub(crate) fn inside_panel(place: Place, point: (f64, f64)) -> bool {
	contains(geometry(place).panel, point)
}
