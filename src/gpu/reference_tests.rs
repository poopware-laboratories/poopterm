//! The software renderer as the GPU renderers' reference: every shared
//! grid scene, over the background or a wallpaper, drawn as quads by a
//! backend and read back, compared with what software paints, within the
//! steps blending may round apart. Each backend runs it on its real GPU.
use crate::{
	gpu::{
		atlas::Slot,
		quads::{Fill, Quad, QuadPen},
	},
	graphics::Bitmap,
	render::{
		background::wallpaper,
		grid::draw_grid,
		grid_case_tests::{BG, SIZE, cases, fonts},
		painter::Painter,
		pen::Pen,
	},
	theme::{Theme, ThemeKind},
};
use std::sync::Arc;

/// How many translucent quads blend into each pixel since the last opaque
/// one covered it: software truncates each blend and the GPU rounds it, so
/// each may leave them a step apart.
pub(crate) fn layers(pen: &QuadPen) -> Vec<u32> {
	let (width, height) = pen.size;
	let mut layers = vec![0; width * height];
	for quad in &pen.quads {
		let (x, y) = quad.at;
		let (w, h) = (quad.size.0 as i32, quad.size.1 as i32);
		let (left, top, right, bottom) = quad.clip;
		// Italic coverage leans right by up to a sixth of its height.
		let lean = if quad.italic && matches!(quad.fill, Fill::Coverage(_)) {
			h / 6
		} else {
			0
		};
		let columns = x.max(left).max(0)..(x + w + lean).min(right).min(width as i32);
		for row in y.max(top).max(0)..(y + h).min(bottom).min(height as i32) {
			for column in columns.clone() {
				let layer = &mut layers[row as usize * width + column as usize];
				*layer = if quad.fill == Fill::Solid {
					0
				} else {
					*layer + 1
				};
			}
		}
	}
	layers
}

/// How many of `drawn`'s pixels differ from `painted`'s in a color channel
/// by more than the `layers` blended into them, and by more than one; the
/// GPU's alpha is not the software's.
pub(crate) fn differing(drawn: &[u32], painted: &[u32], layers: &[u32]) -> usize {
	let close = |a: u32, b: u32, most: u32| {
		(0..3).all(|i| (a >> (i * 8) & 0xff).abs_diff(b >> (i * 8) & 0xff) <= most.max(1))
	};
	let pixels = drawn.iter().zip(painted).zip(layers);
	pixels
		.filter(|&((&a, &b), &most)| !close(a, b, most))
		.count()
}

/// Draws every scene with a pen of `atlas` (side, pages), which `drawn`
/// draws on the GPU and reads back, and says which differ from software,
/// and by how many pixels.
pub(crate) fn unlike_software<E>(
	atlas: (usize, usize),
	mut drawn: impl FnMut(&mut QuadPen) -> Result<Vec<u32>, E>,
) -> Result<Vec<String>, E> {
	let (width, height) = SIZE;
	let (mut pen, mut fonts, mut soft_fonts) = (QuadPen::new(SIZE, atlas), fonts(), fonts());
	let paper = paper();
	// Over a wallpaper, cells of the theme's background show it, as in the
	// app.
	let theme_bg = Theme::from_kind(ThemeKind::Poop).bg;
	let mut unlike = Vec::new();
	for case in cases() {
		for papered in [false, true] {
			let mut view = case.view();
			if papered {
				view.default_bg = theme_bg;
			}
			pen.begin_frame(SIZE);
			let mut painted = vec![BG; width * height];
			let mut painter = Painter::new(&mut painted, width, height);
			// What the software renderer starts from, or the wallpaper
			// drawn alike.
			match papered {
				false => pen.rect(0, 0, width, height, BG),
				true => {
					wallpaper(&mut pen, (0, height), 0, BORDER, EDGE, &paper);
					wallpaper(&mut painter, (0, height), 0, BORDER, EDGE, &paper);
				}
			}
			draw_grid(&mut pen, &mut fonts, &view, None);
			draw_grid(&mut painter, &mut soft_fonts, &view, None);
			let read = drawn(&mut pen)?;
			let differ = differing(&read, &painted, &layers(&pen));
			if differ > 0 {
				let over = if papered { " over a wallpaper" } else { "" };
				unlike.push(format!("{}{over}: {differ} pixels", case.name));
			}
		}
	}
	Ok(unlike)
}

/// The wallpaper's border, narrower than the grid's origin so the grid's
/// first column and row sit over the wallpaper, and its color.
const BORDER: usize = 3;
const EDGE: u32 = 0x60_2040;

/// A wallpaper for the inside of the scenes, as `Wallpaper::bitmap` makes
/// it: opaque, and no two neighbors alike, so a texel off by one shows.
fn paper() -> Arc<Bitmap> {
	let (width, height) = (SIZE.0 - 2 * BORDER, SIZE.1 - 2 * BORDER);
	let pixels = (0..width * height)
		.map(|i| 0xff00_0000 | (i as u32).wrapping_mul(0x9e37_79b9) >> 8)
		.collect();
	Arc::new(Bitmap::new(width, height, pixels))
}

#[test]
fn pixels_differ_by_more_than_their_layers_in_a_color_channel_and_not_in_alpha() {
	let painted = [0x0010_2030; 5];
	let drawn = [
		0xff10_2030,
		0xff11_1f31,
		0x0012_2030,
		0x0010_2230,
		0x0010_2033,
	];
	assert_eq!(differing(&drawn, &painted, &[0; 5]), 3);
	assert_eq!(differing(&drawn, &painted, &[1; 5]), 3);
	// Two layers allow two steps, not three.
	assert_eq!(differing(&drawn, &painted, &[2; 5]), 1);
	assert_eq!(differing(&drawn, &painted, &[3; 5]), 0);
}

#[test]
fn layers_count_translucent_quads_since_the_last_opaque_one() {
	let mut pen = QuadPen::new((8, 4), (16, 1));
	let quad = |at, size, clip, fill, italic| Quad {
		at,
		size,
		clip,
		fill,
		italic,
		color: 0,
	};
	let slot = Slot {
		page: 0,
		x: 0,
		y: 0,
		w: 4,
		h: 4,
	};
	let whole = (0, 0, 8, 4);
	pen.quads = vec![
		quad((-2, 0), (4, 4), whole, Fill::Coverage(slot), false),
		quad((0, 0), (4, 4), (1, 1, 8, 3), Fill::Color(slot), false),
		// Opaque over the first column.
		quad((0, 0), (1, 4), whole, Fill::Solid, false),
		// Leans right by up to 6 / 6 = 1 pixel, and runs off the bottom.
		quad((5, 2), (2, 6), whole, Fill::Coverage(slot), true),
	];
	assert_eq!(
		layers(&pen),
		[
			[0, 1, 0, 0, 0, 0, 0, 0],
			[0, 2, 1, 1, 0, 0, 0, 0],
			[0, 2, 1, 1, 0, 1, 1, 1],
			[0, 1, 0, 0, 0, 1, 1, 1],
		]
		.concat()
	);
}
