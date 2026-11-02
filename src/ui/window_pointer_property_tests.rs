//! Properties of the pointer over a window that draws its own title bar,
//! for every shipped button layout and scale: where edges and buttons win,
//! and press and release sequences against a reference written as a table.
use super::{
	window_controls::{ButtonLayout, Placement, WindowButton, place},
	window_pointer::{PointerTarget, Presses, Route, Surface, pointer_target},
};
use winit::window::ResizeDirection;

const LAYOUTS: [&str; 5] = [
	"appmenu:close",
	"appmenu:minimize,maximize,close",
	"close,minimize,maximize:",
	"close:maximize",
	"appmenu:",
];

fn surfaces() -> Vec<(Placement, Surface, String)> {
	let mut all = Vec::new();
	for scale in 1..=3 {
		for text in LAYOUTS {
			for maximized in [false, true] {
				let size = (900 * scale, 500 * scale);
				let surface = Surface {
					size,
					bar: (0, 44 * scale),
					scale,
					maximized,
				};
				let placement = place(&ButtonLayout::parse(text), size.0, scale);
				all.push((
					placement,
					surface,
					format!("{text} at {scale}, maximized {maximized}"),
				));
			}
		}
	}
	all
}

fn at(placement: &Placement, surface: Surface, (x, y): (usize, usize)) -> Option<PointerTarget> {
	pointer_target(placement, (x as f64, y as f64), surface)
}

#[test]
fn the_outermost_pixels_always_resize_unless_maximized() {
	for (placement, surface, what) in surfaces() {
		let (width, height) = surface.size;
		let ring = (0..width)
			.flat_map(|x| [(x, 0), (x, height - 1)])
			.chain((0..height).flat_map(|y| [(0, y), (width - 1, y)]));
		for point in ring {
			let found = at(&placement, surface, point);
			assert_eq!(
				matches!(found, Some(PointerTarget::Resize(_))),
				!surface.maximized,
				"{what} at {point:?}"
			);
		}
	}
}

#[test]
fn far_from_the_edges_only_buttons_are_found() {
	for (placement, surface, what) in surfaces() {
		let (width, height) = surface.size;
		let zone = 12 * surface.scale;
		for y in (zone..height - zone).step_by(3) {
			for x in (zone..width - zone).step_by(3) {
				let found = at(&placement, surface, (x, y));
				assert!(
					!matches!(found, Some(PointerTarget::Resize(_))),
					"{what} at {x},{y}"
				);
				if y >= surface.bar.1 {
					assert_eq!(found, None, "{what} at {x},{y}");
				}
			}
		}
	}
}

#[test]
fn the_middle_of_every_button_finds_it_in_every_state() {
	for (placement, surface, what) in surfaces() {
		for &(kind, x) in &placement.buttons {
			let point = (x + placement.width / 2, surface.bar.1 / 2);
			assert_eq!(
				at(&placement, surface, point),
				Some(PointerTarget::Button(kind)),
				"{what}"
			);
		}
	}
}

#[test]
fn a_corner_takes_the_outer_pixels_of_a_button_beside_it() {
	// The fallback layout puts close 8 pixels from the right edge, inside
	// the 12 pixel top-right corner: the corner wins there, so a window
	// can always be resized from its corner.
	let surface = Surface {
		size: (900, 500),
		bar: (0, 44),
		scale: 1,
		maximized: false,
	};
	let placement = place(&ButtonLayout::fallback(), 900, 1);
	assert_eq!(
		at(&placement, surface, (890, 8)),
		Some(PointerTarget::Resize(ResizeDirection::NorthEast))
	);
	assert_eq!(
		at(&placement, surface, (887, 8)),
		Some(PointerTarget::Button(WindowButton::Close))
	);
	assert_eq!(
		at(&placement, surface, (890, 12)),
		Some(PointerTarget::Button(WindowButton::Close))
	);
}

/// What `route` should do, from the held button and the event alone.
fn expected(held: Option<WindowButton>, pressed: bool, target: Option<PointerTarget>) -> Route {
	match (pressed, held, target) {
		(true, _, Some(PointerTarget::Resize(edge))) => Route::Resize(edge),
		(true, _, Some(PointerTarget::Button(_))) => Route::Held,
		(true, _, None) => Route::Pass,
		(false, Some(held), Some(PointerTarget::Button(under))) if held == under => Route::Act(held),
		(false, Some(_), _) => Route::Held,
		(false, None, _) => Route::Pass,
	}
}

#[test]
fn random_press_and_release_sequences_follow_the_table() {
	use WindowButton::*;
	let targets = [
		None,
		Some(PointerTarget::Button(Minimize)),
		Some(PointerTarget::Button(Maximize)),
		Some(PointerTarget::Button(Close)),
		Some(PointerTarget::Resize(ResizeDirection::South)),
	];
	// A fixed linear congruential generator: the same sequences every run.
	let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
	let mut next = |bound: usize| {
		seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
		(seed >> 33) as usize % bound
	};
	for run in 0..200 {
		let mut presses = Presses::default();
		let mut held = None;
		let mut acted = 0;
		for step in 0..40 {
			let (pressed, target) = (next(2) == 0, targets[next(targets.len())]);
			let want = expected(held, pressed, target);
			assert_eq!(
				presses.route(pressed, target),
				want,
				"run {run} step {step}"
			);
			held = match (pressed, target) {
				(true, Some(PointerTarget::Button(button))) => Some(button),
				(true, _) => held,
				(false, _) => None,
			};
			assert_eq!(presses.held(), held, "run {run} step {step}");
			acted += usize::from(matches!(want, Route::Act(_)));
		}
		assert!(acted <= 20, "every click needs a press and a release");
	}
}
