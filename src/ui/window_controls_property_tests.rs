//! Properties of the window buttons model that hold for every layout and
//! scale, and the resize edges checked pixel by pixel against a reference
//! written from the spec independently of `resize_edge`.
use super::window_controls::{
	ButtonLayout, Placement, WindowButton, WindowButton::*, button_at, place, resize_edge,
};
use winit::window::ResizeDirection;

/// `button-layout` values in the forms desktops use.
const SHIPPED: [(&str, &[WindowButton], &[WindowButton]); 8] = [
	// GNOME's default: only close, on the right.
	("appmenu:close", &[], &[Close]),
	(
		"appmenu:minimize,maximize,close",
		&[],
		&[Minimize, Maximize, Close],
	),
	// All on the left, as on macOS.
	(
		"close,minimize,maximize:",
		&[Close, Minimize, Maximize],
		&[],
	),
	("close:maximize", &[Close], &[Maximize]),
	(
		"icon:minimize,maximize,close",
		&[],
		&[Minimize, Maximize, Close],
	),
	(
		"menu:minimize,maximize,close",
		&[],
		&[Minimize, Maximize, Close],
	),
	// Spacers between buttons are skipped.
	(
		"close,spacer,minimize:spacer,maximize",
		&[Close, Minimize],
		&[Maximize],
	),
	("close:appmenu", &[Close], &[]),
];

#[test]
fn layouts_in_the_forms_desktops_use_read_as_they_show() {
	for (text, left, right) in SHIPPED {
		let layout = ButtonLayout::parse(text);
		assert_eq!(
			(layout.left.as_slice(), layout.right.as_slice()),
			(left, right),
			"{text}"
		);
	}
}

/// Every shipped layout plus the fallback.
fn layouts() -> Vec<ButtonLayout> {
	let mut all: Vec<_> = SHIPPED
		.iter()
		.map(|(text, ..)| ButtonLayout::parse(text))
		.collect();
	all.push(ButtonLayout::fallback());
	all.push(ButtonLayout::default());
	all
}

/// The pixel columns each part of the header takes: the buttons, the
/// dividers, and the room left for tabs (8 pixels inside the reserves).
fn columns(placement: &Placement, width: usize, scale: usize) -> Vec<(usize, usize, &'static str)> {
	let mut parts: Vec<_> = placement
		.buttons
		.iter()
		.map(|&(_, x)| (x, x + placement.width, "button"))
		.collect();
	parts.extend(
		placement
			.dividers
			.iter()
			.map(|&x| (x, x + scale, "divider")),
	);
	let tabs = (
		placement.left_reserve + 8 * scale,
		width - placement.right_reserve - 8 * scale,
	);
	parts.push((tabs.0, tabs.1, "tabs"));
	parts.sort();
	parts
}

#[test]
fn buttons_dividers_and_tabs_never_overlap_and_stay_inside() {
	for scale in 1..=3 {
		let width = 1100 * scale;
		for layout in layouts() {
			let placement = place(&layout, width, scale);
			let parts = columns(&placement, width, scale);
			for pair in parts.windows(2) {
				assert!(pair[0].1 <= pair[1].0, "{layout:?} at {scale}: {pair:?}");
			}
			assert!(
				parts
					.iter()
					.all(|&(start, end, _)| start >= 8 * scale && end <= width - 8 * scale)
			);
		}
	}
}

#[test]
fn margins_around_each_divider_are_eight_pixels() {
	for scale in 1..=3 {
		let placement = place(&ButtonLayout::parse("close:minimize"), 1100 * scale, scale);
		let [(_, left), (_, right)] = placement.buttons[..] else {
			panic!()
		};
		let [first, second] = placement.dividers[..] else {
			panic!()
		};
		assert_eq!(first - (left + placement.width), 8 * scale);
		assert_eq!(right - (second + scale), 8 * scale);
	}
}

#[test]
fn a_reserve_covers_its_buttons_divider_and_margins() {
	for scale in 1..=3 {
		for count in 1..=3 {
			let right = vec![Close; count];
			let layout = ButtonLayout {
				left: right.clone(),
				right,
			};
			let placement = place(&layout, 1100 * scale, scale);
			let expected = (count * 36 + 16 + 1) * scale;
			assert_eq!(
				(placement.left_reserve, placement.right_reserve),
				(expected, expected)
			);
		}
	}
}

#[test]
fn the_middle_of_every_button_hits_it_at_every_scale() {
	for scale in 1..=3 {
		for layout in layouts() {
			let placement = place(&layout, 1100 * scale, scale);
			for &(button, x) in &placement.buttons {
				let middle = ((x + placement.width / 2) as f64, (22 * scale) as f64);
				assert_eq!(
					button_at(&placement, middle, (0, 44 * scale), scale),
					Some(button)
				);
			}
		}
	}
}

#[test]
fn dividers_margins_and_the_tab_room_hit_nothing() {
	for scale in 1..=3 {
		let width = 1100 * scale;
		for layout in layouts() {
			let placement = place(&layout, width, scale);
			let misses: Vec<_> = columns(&placement, width, scale)
				.into_iter()
				.filter(|part| part.2 != "button")
				.flat_map(|(start, end, _)| [start, end - 1])
				.chain([0, 8 * scale - 1, width - 8 * scale, width - 1])
				.collect();
			for x in misses {
				let point = (x as f64, (22 * scale) as f64);
				assert_eq!(
					button_at(&placement, point, (0, 44 * scale), scale),
					None,
					"{x}"
				);
			}
		}
	}
}

#[test]
fn the_rows_above_and_below_the_buttons_hit_nothing() {
	for scale in 1..=3 {
		let placement = place(&ButtonLayout::fallback(), 1100 * scale, scale);
		let x = (placement.buttons[0].1 + 1) as f64;
		let at = |y: usize| button_at(&placement, (x, y as f64), (0, 44 * scale), scale);
		assert_eq!(at(6 * scale - 1), None);
		assert_eq!(at(6 * scale), Some(Minimize));
		assert_eq!(at(38 * scale - 1), Some(Minimize));
		assert_eq!(at(38 * scale), None);
	}
}

/// The spec, pixel by pixel: how far the pixel at `(x, y)` is from each
/// side, counting the side's own pixel as 0, then corners before sides.
fn reference(x: usize, y: usize, size: (usize, usize), scale: usize) -> Option<ResizeDirection> {
	use ResizeDirection::*;
	let (left, top) = (x, y);
	let (right, bottom) = (size.0 - 1 - x, size.1 - 1 - y);
	let (corner, side) = (12 * scale, 6 * scale);
	// The left and top win when a window is too small to keep them apart.
	let (west, north) = (left < corner, top < corner);
	let (horizontal, vertical) = (left.min(right) < corner, top.min(bottom) < corner);
	if horizontal && vertical {
		return Some(match (west, north) {
			(true, true) => NorthWest,
			(false, true) => NorthEast,
			(true, false) => SouthWest,
			(false, false) => SouthEast,
		});
	}
	if top < side {
		Some(North)
	} else if bottom < side {
		Some(South)
	} else if left < side {
		Some(West)
	} else if right < side {
		Some(East)
	} else {
		None
	}
}

#[test]
fn every_pixel_resizes_as_the_spec_says() {
	for (size, scale) in [
		((300, 200), 1),
		((301, 199), 1),
		((600, 400), 2),
		((90, 90), 3),
	] {
		for y in 0..size.1 {
			for x in 0..size.0 {
				// Both the pixel's start and its middle land in its zone.
				for offset in [0.0, 0.5] {
					let point = (x as f64 + offset, y as f64 + offset);
					assert_eq!(
						resize_edge(point, size, scale, false),
						reference(x, y, size, scale),
						"({x}, {y}) in {size:?} at {scale}"
					);
				}
			}
		}
	}
}
