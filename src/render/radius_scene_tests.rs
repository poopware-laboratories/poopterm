//! Characterization tests of the corner radii each piece of chrome draws,
//! recorded as scenes: they pin the values the drawing code used before
//! `ui::metrics` named them, at every scale.
use super::{
	chrome::button,
	glyphs::{FontSet, Glyph},
	header::{
		Badge, HeaderButton, HeaderColors, HeaderTab, Icon, Style, TabShape, close_rect, draw_button,
		draw_tab,
	},
	label::Label,
	painter::ButtonStyle,
	pen::{Ink, Pen},
	popover::draw_popover,
	scene::{Command, Rect, Scene},
	sidebar::{Sidebar, SidebarRow, draw_sidebar, rail_close_rect},
};
use crate::{
	fonts::{BUNDLED, Font},
	graphics::{Placement, Rect as ImageRect},
	theme::{Theme, ThemeKind},
	ui::overlays::{Place, command_palette, search_box},
};

const SCALES: [usize; 3] = [1, 2, 3];
const COLORS: HeaderColors = HeaderColors {
	bar: 0x202020,
	separator: 0x404040,
	hover: 0x303030,
	raised: 0x383838,
	fg: 0xe0e0e0,
	dim: 0x808080,
};

fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new())
}

fn style(scale: usize) -> Style {
	Style {
		scale,
		px: 13.0 * scale as f32,
		colors: COLORS,
	}
}

/// Every rounded rectangle in `scene`, in drawing order.
fn round_rects(scene: &Scene) -> Vec<(Rect, usize)> {
	let mut found = Vec::new();
	for command in &scene.commands {
		if let Command::RoundRect(rect, radius, _) = *command {
			found.push((rect, radius));
		}
	}
	found
}

fn tab(width: usize, close: Option<bool>) -> HeaderTab<'static> {
	HeaderTab {
		x: 0,
		width,
		label: Label {
			title: "cargo build",
			status: None,
			colors: (COLORS.fg, COLORS.dim),
		},
		fill: Some(COLORS.raised),
		badge: Some(Badge::Dot(0xff8800)),
		shape: TabShape::Row,
		panes: None,
		progress: None,
		close,
	}
}

#[test]
fn an_active_tab_rounds_its_fill_by_six_and_its_dot_into_a_circle() {
	for scale in SCALES {
		let rect = (0, 0, 180 * scale, 32 * scale);
		let mut scene = Scene::new(rect.2, rect.3);
		draw_tab(
			&mut scene,
			&mut fonts(),
			style(scale),
			rect,
			&tab(rect.2, None),
		);
		let rects = round_rects(&scene);
		assert_eq!(rects[0], (rect, 6 * scale), "the fill at {scale}x");
		let (dot, radius) = rects[1];
		assert_eq!((dot.2, dot.3, radius), (6 * scale, 6 * scale, 3 * scale));
		assert_eq!(rects.len(), 2);
	}
}

#[test]
fn a_hovered_close_button_is_a_square_rounded_like_a_chip() {
	for scale in SCALES {
		let rect = (0, 0, 180 * scale, 32 * scale);
		let mut scene = Scene::new(rect.2, rect.3);
		draw_tab(
			&mut scene,
			&mut fonts(),
			style(scale),
			rect,
			&tab(rect.2, Some(true)),
		);
		let (cx, cy, size, _) = close_rect(0, rect.2, (0, rect.3), scale);
		let close = *round_rects(&scene).last().unwrap();
		let chip = crate::ui::metrics::Corner::Chip.radius(scale);
		assert_eq!(close, ((cx, cy, size, size), chip), "at {scale}x");
	}
}

#[test]
fn a_close_button_not_hovered_draws_no_circle() {
	let rect = (0, 0, 180, 32);
	let mut scene = Scene::new(rect.2, rect.3);
	draw_tab(
		&mut scene,
		&mut fonts(),
		style(1),
		rect,
		&tab(rect.2, Some(false)),
	);
	assert_eq!(round_rects(&scene).len(), 2, "fill and dot only");
}

#[test]
fn hovered_and_pressed_header_buttons_round_by_six() {
	for scale in SCALES {
		for pressed in [false, true] {
			let rect = (4 * scale, 4 * scale, 28 * scale, 28 * scale);
			let model = HeaderButton {
				rect,
				icon: Icon::Plus,
				hovered: true,
				enabled: true,
				pressed,
			};
			let mut scene = Scene::new(40 * scale, 40 * scale);
			draw_button(&mut scene, &mut fonts(), style(scale), &model);
			assert_eq!(
				round_rects(&scene),
				[(rect, 6 * scale)],
				"pressed {pressed}"
			);
		}
	}
}

#[test]
fn a_header_button_at_rest_draws_no_fill() {
	let model = HeaderButton {
		rect: (0, 0, 28, 28),
		icon: Icon::Menu,
		hovered: false,
		enabled: true,
		pressed: false,
	};
	let mut scene = Scene::new(28, 28);
	draw_button(&mut scene, &mut fonts(), style(1), &model);
	assert!(round_rects(&scene).is_empty());
}

#[test]
fn rail_tabs_round_like_header_tabs_and_their_close_is_a_circle() {
	for scale in SCALES {
		let row = (4 * scale, 4 * scale, 40 * scale, 36 * scale);
		let sidebar = Sidebar {
			area: (0, 0, 48 * scale, 200 * scale),
			style: style(scale),
			rail: true,
			rows: vec![SidebarRow {
				number: 1,
				rect: row,
				tab: tab(row.2, Some(true)),
			}],
			buttons: Vec::new(),
		};
		let mut scene = Scene::new(48 * scale, 200 * scale);
		draw_sidebar(&mut scene, &mut fonts(), &sidebar);
		let rects = round_rects(&scene);
		assert_eq!(rects[0], (row, 6 * scale), "the fill at {scale}x");
		assert_eq!(rects[1].1, 3 * scale, "the dot at {scale}x");
		let (cx, cy, size, _) = rail_close_rect(row, scale);
		assert_eq!(rects[2], ((cx, cy, size, size), size / 2));
	}
}

#[test]
fn popovers_round_their_frame_by_ten_and_the_fill_inside_by_nine() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	for scale in SCALES {
		let place = Place {
			window: (900 * scale, 500 * scale),
			top: 44 * scale,
			scale,
		};
		let model = command_palette(place, ("", false), 12.0 * scale as f32, &theme);
		let mut scene = Scene::new(place.window.0, place.window.1);
		draw_popover(&mut scene, &mut fonts(), &model);
		let rects = round_rects(&scene);
		assert_eq!(rects[0], (model.rect, 10 * scale), "frame at {scale}x");
		assert_eq!(rects[1].1, 9 * scale, "inner fill at {scale}x");
		let (x, y, w, h) = model.rect;
		assert_eq!(
			rects[1].0,
			(x + scale, y + scale, w - 2 * scale, h - 2 * scale)
		);
		// The field and the selected row sit on the fill, rounded by six.
		assert!(rects[2..].iter().all(|&(_, radius)| radius == 6 * scale));
		assert_eq!(rects.len(), 4, "frame, fill, field, selected row");
	}
}

#[test]
fn the_search_box_rounds_its_field_by_six_at_every_scale() {
	let theme = Theme::from_kind(ThemeKind::MacOsLight);
	for scale in SCALES {
		let place = Place {
			window: (900 * scale, 500 * scale),
			top: 44 * scale,
			scale,
		};
		let model = search_box(place, ("poop", 1, 8), 12.0 * scale as f32, &theme);
		let mut scene = Scene::new(place.window.0, place.window.1);
		draw_popover(&mut scene, &mut fonts(), &model);
		let radii: Vec<_> = round_rects(&scene).into_iter().map(|(_, r)| r).collect();
		assert_eq!(radii, [10 * scale, 9 * scale, 6 * scale]);
	}
}

/// A pen that keeps only the rounded rectangles drawn on it.
struct RoundRects(Vec<(Rect, usize)>);

impl Pen for RoundRects {
	fn size(&self) -> (usize, usize) {
		(400, 400)
	}

	fn rect(&mut self, _x: usize, _y: usize, _w: usize, _h: usize, _color: u32) {}

	fn round_rect(&mut self, rect: Rect, radius: usize, _color: u32) {
		self.0.push((rect, radius));
	}

	fn glyph(&mut self, _: &Glyph, _: Ink, _: (i32, i32), _: (i32, i32, i32, i32), _: bool, _: u32) {}

	fn placement(&mut self, _: ImageRect, _: &Placement, _: (isize, isize), _: (usize, usize)) {}
}

#[test]
fn a_bordered_button_rounds_its_outline_by_seven_and_its_face_by_six() {
	for scale in SCALES {
		for hovered in [false, true] {
			let style = ButtonStyle {
				scale,
				background: 0x202020,
				border: 0x606060,
				accent: 0x0a84ff,
			};
			let mut pen = RoundRects(Vec::new());
			let rect = (10, 10, 120 * scale, 32 * scale);
			button(&mut pen, rect, style, hovered);
			let face = (
				10 + scale,
				10 + scale,
				rect.2 - 2 * scale,
				rect.3 - 2 * scale,
			);
			assert_eq!(
				pen.0,
				[(rect, 7 * scale), (face, 6 * scale)],
				"hovered {hovered}"
			);
		}
	}
}
