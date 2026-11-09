//! Tests of the search box's controls: where the field and the four
//! buttons sit, which one a point is on, and what each shows.
use super::{
	overlays::Place,
	search_controls::{BUTTON, SearchControl, control_at, geometry, inside_panel, needs_buttons},
};

fn place(window: (usize, usize), scale: usize) -> Place {
	Place {
		window,
		top: 44 * scale,
		scale,
	}
}

const ORDER: [SearchControl; 4] = [
	SearchControl::Previous,
	SearchControl::Next,
	SearchControl::Case,
	SearchControl::Close,
];

#[test]
fn the_panel_keeps_the_place_the_search_box_always_had() {
	let geometry = geometry(place((1000, 600), 1));
	assert_eq!(geometry.panel, (608, 52, 380, 50));
}

#[test]
fn the_field_leaves_room_for_four_buttons_at_its_right() {
	let geometry = geometry(place((1000, 600), 1));
	assert_eq!(geometry.field, (616, 60, 242, 34));
	let controls: Vec<_> = geometry
		.buttons
		.iter()
		.map(|&(control, _)| control)
		.collect();
	assert_eq!(controls, ORDER);
}

#[test]
fn the_buttons_are_square_two_pixels_apart_and_centered_on_the_field() {
	let geometry = geometry(place((1000, 600), 1));
	let rects: Vec<_> = geometry.buttons.iter().map(|&(_, rect)| rect).collect();
	assert_eq!(
		rects,
		[
			(862, 63, 28, 28),
			(892, 63, 28, 28),
			(922, 63, 28, 28),
			(952, 63, 28, 28)
		]
	);
	assert_eq!(BUTTON, 28);
}

#[test]
fn the_last_button_ends_at_the_panels_inset_edge() {
	for window in [(1000, 600), (700, 500), (420, 300)] {
		let geometry = geometry(place(window, 1));
		let (_, (x, _, w, _)) = *geometry.buttons.last().unwrap();
		let (px, _, pw, _) = geometry.panel;
		assert_eq!(x + w, px + pw - 8, "{window:?}");
	}
}

#[test]
fn the_field_and_buttons_never_overlap() {
	for window in [(1000, 600), (700, 500), (420, 300), (330, 200)] {
		for scale in [1, 2] {
			let geometry = geometry(place((window.0 * scale, window.1 * scale), scale));
			let field_end = geometry.field.0 + geometry.field.2;
			for &(_, rect) in &geometry.buttons {
				assert!(rect.0 >= field_end, "{window:?} at {scale}x");
			}
		}
	}
}

#[test]
fn everything_doubles_at_twice_the_scale() {
	let one = geometry(place((1000, 600), 1));
	let two = geometry(place((2000, 1200), 2));
	assert_eq!(two.field.2, one.field.2 * 2);
	for (a, b) in one.buttons.iter().zip(&two.buttons) {
		assert_eq!((b.1.2, b.1.3), (a.1.2 * 2, a.1.3 * 2));
	}
}

#[test]
fn a_window_too_narrow_for_the_buttons_keeps_a_plain_field() {
	let tiny = geometry(place((200, 400), 1));
	assert!(tiny.buttons.is_empty());
	assert_eq!(tiny.field.2, tiny.panel.2.saturating_sub(16));
	assert!(!needs_buttons(tiny.panel.2, 1));
	assert!(needs_buttons(380, 1));
}

#[test]
fn a_window_with_no_room_has_no_panel_and_no_buttons() {
	let none = geometry(place((10, 600), 1));
	assert_eq!((none.panel.0, none.panel.2), (0, 0));
	assert!(none.buttons.is_empty());
	assert_eq!(none.field.2, 0);
}

#[test]
fn a_point_on_a_button_names_it() {
	let place = place((1000, 600), 1);
	for (control, (x, y, w, h)) in geometry(place).buttons {
		for point in [(x, y), (x + w - 1, y + h - 1), (x + w / 2, y + h / 2)] {
			let point = (point.0 as f64, point.1 as f64);
			assert_eq!(control_at(place, point), Some(control), "{control:?}");
		}
	}
}

#[test]
fn a_point_in_the_gap_or_off_the_buttons_names_none() {
	let place = place((1000, 600), 1);
	assert_eq!(control_at(place, (890.5, 70.0)), None, "the gap");
	assert_eq!(control_at(place, (870.0, 62.0)), None, "above a button");
	assert_eq!(control_at(place, (870.0, 91.0)), None, "below a button");
	assert_eq!(control_at(place, (700.0, 70.0)), None, "over the field");
	assert_eq!(control_at(place, (-1.0, -1.0)), None);
	assert_eq!(control_at(place, (f64::NAN, 70.0)), None);
}

#[test]
fn the_panel_holds_its_points_and_nothing_outside() {
	let place = place((1000, 600), 1);
	assert!(inside_panel(place, (608.0, 52.0)));
	assert!(inside_panel(place, (987.9, 101.9)));
	assert!(!inside_panel(place, (988.0, 70.0)));
	assert!(!inside_panel(place, (607.9, 70.0)));
	assert!(!inside_panel(place, (700.0, 102.0)));
	assert!(!inside_panel(place, (700.0, 51.9)));
}

#[test]
fn the_controls_keep_their_order_and_each_has_a_distinct_glyph() {
	assert_eq!(SearchControl::ALL, ORDER);
	let glyphs: Vec<_> = ORDER.iter().map(|control| control.glyph()).collect();
	assert_eq!(glyphs, ["↑", "↓", "Aa", "×"]);
}

#[test]
fn at_every_window_size_and_scale_the_parts_sit_inside_each_other_in_order() {
	for scale in 1..=3 {
		for width in (0..1600).step_by(13) {
			let window = (width * scale, 700 * scale);
			let geometry = geometry(place(window, scale));
			let (px, _, pw, _) = geometry.panel;
			assert!(px + pw <= window.0, "panel in {width} at {scale}x");
			let (fx, _, fw, _) = geometry.field;
			// A field with no width draws nothing, wherever it starts.
			assert!(
				fw == 0 || fx >= px && fx + fw <= px + pw,
				"field in {width} at {scale}x"
			);
			let mut edge = fx + fw;
			for &(_, (x, _, w, h)) in &geometry.buttons {
				assert!(
					x >= edge,
					"a button before the one ahead of it, {width} at {scale}x"
				);
				assert_eq!((w, h), (28 * scale, 28 * scale), "{width} at {scale}x");
				edge = x + w + 2 * scale;
				assert!(
					x + w <= px + pw - 8 * scale,
					"inside the panel, {width} at {scale}x"
				);
			}
		}
	}
}

#[test]
fn the_buttons_sit_in_the_fields_vertical_band_at_every_scale() {
	for scale in 1..=3 {
		let geometry = geometry(place((1200 * scale, 700 * scale), scale));
		let (_, fy, _, fh) = geometry.field;
		for &(_, (_, y, _, h)) in &geometry.buttons {
			assert_eq!(y - fy, 3 * scale, "{scale}x");
			assert_eq!(fy + fh - (y + h), 3 * scale, "{scale}x");
		}
	}
}

#[test]
fn sweeping_across_the_row_each_point_names_at_most_one_button_in_order() {
	for scale in 1..=2 {
		let place = place((1000 * scale, 600 * scale), scale);
		let geometry = geometry(place);
		let (_, y, _, h) = geometry.buttons[0].1;
		let row = (y + h / 2) as f64;
		let mut seen = Vec::new();
		for x in 0..1000 * scale {
			if let Some(control) = control_at(place, (x as f64 + 0.5, row))
				&& seen.last() != Some(&control)
			{
				seen.push(control);
			}
		}
		assert_eq!(seen, ORDER, "{scale}x");
	}
}

#[test]
fn every_pixel_of_a_button_hits_it_and_the_pixels_around_it_do_not() {
	let place = place((1000, 600), 1);
	for (control, (x, y, w, h)) in geometry(place).buttons {
		for row in y - 1..=y + h {
			for column in x - 1..=x + w {
				let inside = (x..x + w).contains(&column) && (y..y + h).contains(&row);
				let hit = control_at(place, (column as f64 + 0.5, row as f64 + 0.5));
				assert_eq!(
					hit == Some(control),
					inside,
					"{control:?} at ({column}, {row})"
				);
			}
		}
	}
}

#[test]
fn the_panel_is_hit_exactly_where_the_search_box_is_drawn() {
	let place = place((1000, 600), 1);
	let (x, y, w, h) = geometry(place).panel;
	for row in y - 1..=y + h {
		for column in x - 1..=x + w {
			let inside = (x..x + w).contains(&column) && (y..y + h).contains(&row);
			let point = (column as f64 + 0.5, row as f64 + 0.5);
			assert_eq!(inside_panel(place, point), inside, "({column}, {row})");
		}
	}
}

#[test]
fn a_button_is_always_inside_the_panel_that_swallows_presses() {
	for width in (150..1500).step_by(37) {
		let place = place((width, 600), 1);
		for (_, (x, y, w, h)) in geometry(place).buttons {
			for point in [(x, y), (x + w - 1, y + h - 1)] {
				let point = (point.0 as f64 + 0.5, point.1 as f64 + 0.5);
				assert!(inside_panel(place, point), "{width}");
			}
		}
	}
}

#[test]
fn the_buttons_appear_at_exactly_the_width_that_leaves_the_field_a_hundred_pixels() {
	// 16 inset + 118 of buttons + 4 gap + 100 field.
	assert!(!needs_buttons(237, 1));
	assert!(needs_buttons(238, 1));
	assert!(!needs_buttons(475, 2));
	assert!(needs_buttons(476, 2));
	assert!(!needs_buttons(0, 1));
	assert!(needs_buttons(usize::MAX / 4, 1));
}

#[test]
fn whenever_the_buttons_show_the_field_keeps_at_least_a_hundred_pixels() {
	for scale in 1..=3 {
		for width in (0..1500).step_by(5) {
			let geometry = geometry(place((width * scale, 700 * scale), scale));
			if !geometry.buttons.is_empty() {
				assert!(geometry.field.2 >= 100 * scale, "{width} at {scale}x");
			}
		}
	}
}

#[test]
fn the_buttons_appear_and_vanish_together() {
	for width in (0..1500).step_by(3) {
		let count = geometry(place((width, 700), 1)).buttons.len();
		assert!(count == 0 || count == 4, "{count} at {width}");
	}
}

#[test]
fn a_panel_just_wide_enough_for_the_buttons_leaves_the_field_its_minimum() {
	let narrowest = (0..1000)
		.find(|&width| needs_buttons(width, 1))
		.expect("some width has buttons");
	let window = narrowest + 2 * 12;
	let geometry = geometry(place((window, 600), 1));
	assert_eq!(geometry.panel.2, narrowest);
	assert_eq!(geometry.field.2, 100);
}
