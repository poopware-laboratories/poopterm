//! Tests of the chrome's corner radii: each kind keeps today's value, scales
//! with the display, and nests concentrically inside its outline.
use super::metrics::{Corner, concentric, pill_radius};

const KINDS: [Corner; 3] = [Corner::Control, Corner::Button, Corner::Popover];

#[test]
fn each_kind_keeps_the_radius_drawn_before_the_table_existed() {
	assert_eq!(Corner::Control.radius(1), 6);
	assert_eq!(Corner::Button.radius(1), 7);
	assert_eq!(Corner::Popover.radius(1), 10);
}

#[test]
fn radii_grow_linearly_with_the_scale_factor() {
	for kind in KINDS {
		for scale in 1..=4 {
			assert_eq!(
				kind.radius(scale),
				kind.radius(1) * scale,
				"{kind:?} at {scale}x"
			);
		}
	}
}

#[test]
fn a_zero_scale_rounds_nothing() {
	for kind in KINDS {
		assert_eq!(kind.radius(0), 0, "{kind:?}");
	}
}

#[test]
fn larger_surfaces_get_rounder_corners_than_what_sits_on_them() {
	// A control sits on a popover and a button face sits inside its outline,
	// so the containing kind must be the rounder one at every scale.
	for scale in 1..=4 {
		assert!(Corner::Control.radius(scale) < Corner::Button.radius(scale));
		assert!(Corner::Button.radius(scale) < Corner::Popover.radius(scale));
	}
}

#[test]
fn a_fill_inset_by_one_pixel_keeps_the_outlines_curve() {
	assert_eq!(concentric(6, 1), 5);
	assert_eq!(concentric(7, 1), 6);
	assert_eq!(concentric(10, 1), 9);
}

#[test]
fn concentric_radius_follows_the_inset_at_every_scale() {
	for kind in KINDS {
		for scale in 1..=4 {
			let outer = kind.radius(scale);
			// One device pixel of outline per unit of scale, as the painters draw it.
			assert_eq!(
				concentric(outer, scale) + scale,
				outer,
				"{kind:?} at {scale}x"
			);
		}
	}
}

#[test]
fn an_inset_deeper_than_the_radius_leaves_square_corners() {
	assert_eq!(concentric(6, 6), 0);
	assert_eq!(concentric(6, 9), 0);
	assert_eq!(concentric(0, 1), 0);
}

#[test]
fn a_pill_is_rounded_by_half_its_side() {
	assert_eq!(pill_radius(6), 3);
	assert_eq!(pill_radius(12), 6);
	assert_eq!(pill_radius(16), 8);
}

#[test]
fn an_odd_pill_rounds_down_so_its_arcs_never_overlap() {
	// Two radii wider than the side would make the corners cross.
	for side in [1, 3, 5, 7, 13] {
		assert!(2 * pill_radius(side) <= side, "side {side}");
		assert_eq!(pill_radius(side), side / 2);
	}
}

#[test]
fn an_empty_pill_has_no_radius() {
	assert_eq!(pill_radius(0), 0);
}
