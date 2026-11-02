//! Window buttons in the Linux header: which ones the desktop asks for and
//! on which side, where they sit, what a press hits, and the resize edges.
use winit::window::ResizeDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowButton {
	Minimize,
	Maximize,
	Close,
}
use WindowButton::*;

/// The buttons at each end of the header, in drawing order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ButtonLayout {
	pub left: Vec<WindowButton>,
	pub right: Vec<WindowButton>,
}

/// The names GNOME's `button-layout` gives the buttons we draw.
const NAMES: [(&str, WindowButton); 3] = [
	("minimize", Minimize),
	("maximize", Maximize),
	("close", Close),
];

impl ButtonLayout {
	/// All three on the right, for desktops that do not say.
	pub(crate) fn fallback() -> Self {
		Self {
			left: Vec::new(),
			right: vec![Minimize, Maximize, Close],
		}
	}

	/// Reads a GNOME `button-layout` value such as `appmenu:minimize,close`:
	/// the buttons before the colon go left, those after it right. Names we
	/// do not draw (`appmenu`, `icon`, `spacer`) are skipped, and each
	/// button is kept only where it is first named.
	pub(crate) fn parse(text: &str) -> Self {
		let mut sides: [Vec<WindowButton>; 2] = Default::default();
		for (side, part) in text.split(':').take(2).enumerate() {
			for name in part.split(',') {
				let known = NAMES.iter().find(|(known, _)| *known == name.trim());
				if let Some(&(_, button)) = known
					&& !sides.iter().flatten().any(|&placed| placed == button)
				{
					sides[side].push(button);
				}
			}
		}
		let [left, right] = sides;
		Self { left, right }
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
	/// Each button and its left edge; all are `width` wide.
	pub buttons: Vec<(WindowButton, usize)>,
	pub width: usize,
	/// The x of each divider between the buttons and the tabs, `scale`
	/// pixels wide.
	pub dividers: Vec<usize>,
	/// How much of each end the buttons and their divider take.
	pub left_reserve: usize,
	pub right_reserve: usize,
}

/// Unscaled lengths: buttons are as wide as the new tab button, 8 pixels
/// from the window edge, and 8 pixels either side of their divider.
const BUTTON: usize = 36;
const BUTTON_HEIGHT: usize = 32;
const MARGIN: usize = 8;

/// Places `layout`'s buttons at both ends of a `width` pixel header.
pub(crate) fn place(layout: &ButtonLayout, width: usize, scale: usize) -> Placement {
	let (button, margin) = (BUTTON * scale, MARGIN * scale);
	// The buttons, their margins on both sides, and the divider.
	let reserve = |count: usize| (count > 0).then(|| count * button + 2 * margin + scale);
	let (left, right) = (reserve(layout.left.len()), reserve(layout.right.len()));
	let right_start = width.saturating_sub(right.unwrap_or(0).saturating_sub(margin + scale));
	let row = |buttons: &[WindowButton], start: usize| {
		let at = move |(index, &kind): (usize, _)| (kind, start + index * button);
		buttons.iter().enumerate().map(at).collect::<Vec<_>>()
	};
	Placement {
		buttons: [row(&layout.left, margin), row(&layout.right, right_start)].concat(),
		width: button,
		dividers: [
			left.map(|r| r - scale),
			right.map(|r| width.saturating_sub(r)),
		]
		.into_iter()
		.flatten()
		.collect(),
		left_reserve: left.unwrap_or(0),
		right_reserve: right.unwrap_or(0),
	}
}

/// The button under `point` in a header bar at `bar` (top and height),
/// whose buttons are 32 pixels tall and centred in it.
pub(crate) fn button_at(
	placement: &Placement,
	point: (f64, f64),
	bar: (usize, usize),
	scale: usize,
) -> Option<WindowButton> {
	let height = BUTTON_HEIGHT * scale;
	let top = bar.0 + bar.1.saturating_sub(height) / 2;
	let within =
		|at: f64, start: usize, length: usize| at >= start as f64 && at < (start + length) as f64;
	let mut hit = placement
		.buttons
		.iter()
		.filter(|&&(_, x)| within(point.0, x, placement.width));
	hit
		.next()
		.filter(|_| within(point.1, top, height))
		.map(|&(button, _)| button)
}

/// The edge a press at `point` resizes a `size` window from: 6 pixels along
/// each side, 12 pixels along each side at a corner. A maximized window
/// does not resize.
pub(crate) fn resize_edge(
	point: (f64, f64),
	size: (usize, usize),
	scale: usize,
	maximized: bool,
) -> Option<ResizeDirection> {
	let (x, y) = point;
	let (width, height) = (size.0 as f64, size.1 as f64);
	if maximized || !(0.0..width).contains(&x) || !(0.0..height).contains(&y) {
		return None;
	}
	// -1 within `zone` of the left or top, 1 within it of the right or
	// bottom (`length - at` falls to just above 0 on the last pixel), else 0.
	let side = |at: f64, length: f64, zone: usize| {
		let zone = (zone * scale) as f64;
		if at < zone {
			-1
		} else {
			i8::from(length - at <= zone)
		}
	};
	let corner = (side(x, width, 12), side(y, height, 12));
	let near = if corner.0 != 0 && corner.1 != 0 {
		corner
	} else {
		(side(x, width, 6), side(y, height, 6))
	};
	use ResizeDirection::*;
	Some(match near {
		(-1, -1) => NorthWest,
		(1, -1) => NorthEast,
		(-1, 1) => SouthWest,
		(1, 1) => SouthEast,
		(0, -1) => North,
		(0, 1) => South,
		(-1, 0) => West,
		(1, 0) => East,
		_ => return None,
	})
}
