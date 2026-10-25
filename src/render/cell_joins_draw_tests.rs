//! Drawing emoji joined across cells: flags and skin tones as one glyph
//! over both cells, pairs the font does not join, the second cell's
//! background, selections, and glyphs too wide for their cells.
use crate::{
	fonts::{
		Font,
		cbdt_tests::{Image, Strike, font_tables, png, sfnt},
		gsub_tests::{gsub, ligatures, lookup},
	},
	render::glyphs::{
		FontSet,
		clusters::tests::{
			BLUE, GREEN, RED,
			grid::{Grid, SELECTED, bundled},
		},
	},
	ui::selection::{CellPoint, Selection},
};

const MAGENTA: u32 = 0xffff_00ff;

/// The characters of [`joining_font`], sorted, as glyphs 1 to 7.
const CHARS: [char; 7] = ['\u{200d}', '🇦', '🇧', '🏽', '👍', '👩', '🚀'];

/// A color font whose characters are blue 10-pixel squares at 16 pixels
/// per em, joining 🇦🇧 into a green flag, 👍🏽 into a red one and 👩🏽‍🚀
/// into a magenta one, each 20 pixels wide.
pub(crate) fn joining_font() -> Font {
	let square = || Image {
		metrics: [10, 10, 0, 14, 12],
		png: png(10, 10, &[BLUE; 100]),
	};
	let wide = |color| Image {
		metrics: [20, 10, 0, 14, 22],
		png: png(20, 10, &[color; 200]),
	};
	let mut images: Vec<_> = CHARS.iter().map(|_| Some(square())).collect();
	images.extend([GREEN, RED, MAGENTA].map(|color| Some(wide(color))));
	let strike = Strike {
		ppem: 16,
		first: 1,
		index_format: 1,
		image_format: 17,
		images,
	};
	let joins = ligatures(&[
		(2, &[(8, &[3][..])]),
		(5, &[(9, &[4][..])]),
		(6, &[(10, &[4, 1, 7][..])]),
	]);
	let mut tables = font_tables(&CHARS, &[strike]);
	tables.push((
		b"GSUB",
		gsub(&[(b"ccmp", &[0])], vec![lookup(4, 0, vec![joins])]),
	));
	tables.sort_by_key(|table| *table.0);
	Font::from_bytes(sfnt(&tables), 0).unwrap()
}

/// `grid` with `input` drawn using [`joining_font`].
fn draw(grid: &Grid, input: &str) -> Vec<u32> {
	let mut fonts = FontSet::new(bundled(), vec![joining_font()]);
	grid.draw_with(&mut fonts, input)
}

/// Grid columns of 12 by 20 pixels, at 16 pixels per em.
fn grid() -> Grid {
	Grid::new(6, (12, 20), 16.0)
}

#[test]
fn a_flag_is_drawn_over_both_of_its_cells() {
	let grid = grid();
	let pixels = draw(&grid, "🇦🇧");
	assert!(grid.has(&pixels, 0, GREEN) && grid.has(&pixels, 1, GREEN));
	assert!(!grid.has(&pixels, 0, BLUE) && !grid.has(&pixels, 1, BLUE));
}

#[test]
fn indicators_the_font_does_not_join_are_drawn_one_by_one() {
	let grid = grid();
	let pixels = draw(&grid, "🇧🇦");
	assert!(grid.has(&pixels, 0, BLUE) && grid.has(&pixels, 1, BLUE));
	assert!(!grid.has(&pixels, 0, GREEN));
}

#[test]
fn an_odd_indicator_after_a_flag_is_drawn_alone() {
	let grid = grid();
	let pixels = draw(&grid, "🇦🇧🇦");
	assert!(grid.has(&pixels, 1, GREEN));
	assert!(grid.has(&pixels, 2, BLUE) && !grid.has(&pixels, 2, GREEN));
	let shifted = draw(&grid, "🇦🇦🇧");
	assert!(grid.has(&pixels, 0, GREEN) && !grid.has(&shifted, 0, GREEN));
	assert!(
		!grid.has(&shifted, 2, GREEN),
		"🇦🇦 is the pair, and 🇧 stays alone"
	);
}

#[test]
fn a_skin_tone_colors_the_emoji_before_it() {
	let grid = grid();
	let pixels = draw(&grid, "👍🏽");
	assert!(grid.has(&pixels, 0, RED) && grid.has(&pixels, 1, RED));
	for col in 0..4 {
		assert!(!grid.has(&pixels, col, BLUE), "column {col}");
	}
}

#[test]
fn a_toned_zwj_sequence_joins_across_cells() {
	let grid = grid();
	let pixels = draw(&grid, "👩🏽\u{200d}🚀");
	assert!(grid.has(&pixels, 0, MAGENTA));
	for col in 0..4 {
		assert!(!grid.has(&pixels, col, BLUE), "column {col}");
		assert!(!grid.has(&pixels, col, RED), "column {col}");
	}
}

#[test]
fn the_second_cell_keeps_its_own_background() {
	let grid = grid();
	let pixels = draw(&grid, "🇦\x1b[48;2;128;64;32m🇧");
	assert!(grid.has(&pixels, 1, 0x80_4020));
	assert!(grid.has(&pixels, 1, GREEN), "the flag is drawn over it");
	assert!(!grid.has(&pixels, 0, 0x80_4020));
}

#[test]
fn a_selected_second_cell_is_highlighted_under_the_flag() {
	let mut grid = grid();
	let point = CellPoint::new(0, 1);
	grid.selection = Some(Selection {
		start: point,
		end: point,
		block: false,
	});
	let pixels = draw(&grid, "🇦🇧");
	assert!(grid.has(&pixels, 1, SELECTED) && grid.has(&pixels, 1, GREEN));
	assert!(!grid.has(&pixels, 0, SELECTED));
}

#[test]
fn a_joined_glyph_wider_than_both_cells_shrinks_into_them() {
	// Two 6-pixel cells hold 12 pixels: the 20-pixel flag scales down and
	// stays out of the third cell, even when it is blank.
	let grid = Grid::new(4, (6, 20), 16.0);
	let pixels = draw(&grid, "🇦🇧");
	assert!(grid.has(&pixels, 0, GREEN) && grid.has(&pixels, 1, GREEN));
	assert!(!grid.has(&pixels, 2, GREEN));
	assert!(!grid.has(&pixels, 0, BLUE));
}

#[test]
fn text_around_a_flag_is_untouched() {
	let grid = grid();
	let alone = draw(&grid, "x  y");
	let around = draw(&grid, "x🇦🇧y");
	for col in [0, 3] {
		assert_eq!(
			grid.cell_colors(&around, col),
			grid.cell_colors(&alone, col),
			"column {col}"
		);
	}
}

/// `input` drawn with Noto Color Emoji, when it is installed.
fn draw_noto(grid: &Grid, input: &str) -> Option<Vec<u32>> {
	let noto = crate::fonts::gsub_tests::noto()?;
	let mut fonts = FontSet::new(bundled(), vec![noto]);
	Some(grid.draw_with(&mut fonts, input))
}

#[test]
fn noto_draws_flags_across_their_two_cells() {
	let grid = Grid::new(4, (12, 24), 18.0);
	let Some(us) = draw_noto(&grid, "🇺🇸") else {
		return;
	};
	let japan = draw_noto(&grid, "🇯🇵").unwrap();
	let lone = draw_noto(&grid, " 🇸").unwrap();
	assert_ne!(us, japan);
	assert_ne!(
		grid.cell_colors(&us, 1),
		grid.cell_colors(&lone, 1),
		"the second cell is part of the flag, not the letter S"
	);
	assert!(grid.cell_colors(&us, 1).iter().any(|&pixel| pixel != 0));
}

#[test]
fn noto_draws_each_skin_tone_on_the_emoji_before_it() {
	let grid = Grid::new(6, (12, 24), 18.0);
	let Some(plain) = draw_noto(&grid, "👍") else {
		return;
	};
	let mut seen = vec![plain];
	for tone in ['🏻', '🏼', '🏽', '🏾', '🏿'] {
		let toned = draw_noto(&grid, &format!("👍{tone}")).unwrap();
		assert!(!seen.contains(&toned), "{tone}");
		for col in 2..4 {
			assert!(
				grid
					.cell_colors(&toned, col)
					.iter()
					.all(|&pixel| pixel == 0),
				"{tone} draws no swatch of its own"
			);
		}
		seen.push(toned);
	}
}

#[test]
fn an_indicator_in_a_block_of_scaled_text_joins_nothing_after_it() {
	// The block draws its own text; the 🇧 after it must still be drawn.
	let grid = grid();
	let pixels = draw(&grid, "\x1b]66;w=1;🇦\x07🇧");
	assert!(grid.has(&pixels, 1, BLUE), "the 🇧 is drawn");
	assert!(!grid.has(&pixels, 0, GREEN) && !grid.has(&pixels, 1, GREEN));
}
