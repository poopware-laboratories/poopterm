//! Joined clusters on the grid: oversized and narrow cells, the cells
//! around a cluster, links and selections over one, unjoined tag
//! sequences, and the cache across frames.
use super::{BLUE, GREEN, RED, emoji_font};
use crate::{
	parser::Parser,
	render::{
		glyphs::FontSet,
		grid::{GridView, draw_grid},
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
	ui::selection::{CellPoint, Selection},
};

const WHITE: u32 = 0xff_ffff;
const LINK: u32 = 0x12_3456;
const SELECTED: u32 = 0x33_4455;

/// A grid of `cols` cells of `cell` pixels, drawn at `px`, with the test
/// emoji font as the fallback.
struct Grid {
	cols: usize,
	cell: (usize, usize),
	px: f32,
	selection: Option<Selection>,
}

impl Grid {
	fn new(cols: usize, cell: (usize, usize), px: f32) -> Self {
		Self {
			cols,
			cell,
			px,
			selection: None,
		}
	}

	fn width(&self) -> usize {
		self.cols * self.cell.0
	}

	/// Draws `input` (after white on black) with `fonts`.
	fn draw_with(&self, fonts: &mut FontSet, input: &str) -> Vec<u32> {
		let theme = Theme::from_kind(ThemeKind::Poop);
		let mut term = Term::new(self.cols, 1, theme, "/".into());
		let input = format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m{input}");
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		frame.cursor = None;
		let mut pixels = vec![0; self.width() * self.cell.1];
		let mut painter = Painter::new(&mut pixels, self.width(), self.cell.1);
		let view = GridView {
			frame: &frame,
			origin: (0, 0),
			cell: self.cell,
			px: self.px,
			ascent: 14,
			scale: 1,
			default_bg: 0,
			link_color: LINK,
			selection_colors: (WHITE, SELECTED),
			match_colors: (0, 0),
			selection: self.selection,
			search_match: None,
			focused: true,
			padding: None,
		};
		draw_grid(&mut painter, fonts, &view, None);
		pixels
	}

	fn draw(&self, input: &str) -> Vec<u32> {
		let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
		self.draw_with(&mut fonts, input)
	}

	/// The colors (without alpha) in cell `col`.
	fn cell_colors(&self, pixels: &[u32], col: usize) -> Vec<u32> {
		let (w, h) = self.cell;
		(0..h)
			.flat_map(|y| pixels[y * self.width() + col * w..][..w].iter())
			.map(|pixel| pixel & 0xff_ffff)
			.collect()
	}

	fn has(&self, pixels: &[u32], col: usize, color: u32) -> bool {
		self.cell_colors(pixels, col).contains(&(color & 0xff_ffff))
	}
}

fn bundled() -> crate::fonts::Font {
	crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap()
}

const KEYCAP: &str = "1\u{fe0f}\u{20e3}";

#[test]
fn a_cluster_in_a_narrow_cell_is_scaled_down_whole() {
	// 10-pixel squares in 6-pixel cells: the keycap shrinks to fit and
	// stays the ligature, never the digit's glyph.
	let grid = Grid::new(3, (6, 20), 16.0);
	let pixels = grid.draw(&format!("{KEYCAP}xx"));
	assert!(grid.has(&pixels, 0, GREEN));
	assert!(!grid.has(&pixels, 0, BLUE));
	assert!(!grid.has(&pixels, 1, GREEN), "shrunk into its own cell");
}

#[test]
fn an_oversized_cluster_spills_into_a_blank_neighbor_only() {
	let grid = Grid::new(3, (6, 20), 16.0);
	let spilled = grid.draw(KEYCAP);
	let blocked = grid.draw(&format!("{KEYCAP}x"));
	let green = |pixels: &[u32], col| {
		grid
			.cell_colors(pixels, col)
			.iter()
			.filter(|&&pixel| pixel == GREEN & 0xff_ffff)
			.count()
	};
	assert!(green(&spilled, 1) > 0, "drawn on into the blank cell");
	assert_eq!(green(&blocked, 1), 0);
	assert!(
		green(&spilled, 0) + green(&spilled, 1) > green(&blocked, 0),
		"drawn larger"
	);
}

#[test]
fn a_cluster_mid_row_draws_in_its_own_cell_and_leaves_the_others() {
	let grid = Grid::new(4, (10, 20), 16.0);
	let pixels = grid.draw(&format!("ab{KEYCAP}c"));
	assert!(grid.has(&pixels, 2, GREEN));
	for col in [0, 1, 3] {
		assert!(!grid.has(&pixels, col, GREEN), "{col}");
		assert!(grid.has(&pixels, col, WHITE), "{col} drawn in text");
	}
}

#[test]
fn two_clusters_side_by_side_each_draw_their_ligature() {
	let grid = Grid::new(4, (10, 20), 16.0);
	let pixels = grid.draw(&format!("{KEYCAP}❤\u{fe0f}\u{200d}🔥"));
	assert!(grid.has(&pixels, 0, GREEN) && !grid.has(&pixels, 0, RED));
	assert!(grid.has(&pixels, 1, RED) && !grid.has(&pixels, 1, GREEN));
}

#[test]
fn a_link_over_a_cluster_is_still_underlined() {
	let grid = Grid::new(4, (10, 20), 16.0);
	let pixels = grid.draw(&format!(
		"\x1b]8;;https://example.com\x07{KEYCAP}\x1b]8;;\x07"
	));
	assert!(grid.has(&pixels, 0, GREEN));
	assert!(grid.has(&pixels, 0, LINK));
	assert!(!grid.has(&pixels, 1, LINK), "only the linked cell");
}

#[test]
fn a_selected_cluster_keeps_its_colors_on_the_selection_background() {
	let mut grid = Grid::new(4, (10, 20), 16.0);
	grid.selection = Some(Selection {
		start: CellPoint { row: 0, col: 0 },
		end: CellPoint { row: 0, col: 0 },
		block: false,
	});
	let pixels = grid.draw(KEYCAP);
	assert!(
		grid.has(&pixels, 0, GREEN),
		"color glyphs ignore the foreground"
	);
	assert!(grid.has(&pixels, 0, SELECTED));
	assert!(!grid.has(&pixels, 1, SELECTED));
}

#[test]
fn a_tag_sequence_no_font_joins_draws_only_its_base() {
	// A flag tag sequence on 🔥: no ligature, and the tag characters are
	// invisible, so the cell holds the plain emoji and nothing else.
	let grid = Grid::new(4, (20, 20), 16.0);
	let pixels = grid.draw("🔥\u{e0067}\u{e0062}\u{e007f}");
	let plain = grid.draw("🔥");
	assert!(grid.has(&pixels, 0, BLUE));
	assert_eq!(grid.cell_colors(&pixels, 0), grid.cell_colors(&plain, 0));
}

#[test]
fn a_cluster_drawn_again_comes_from_the_cache() {
	let grid = Grid::new(4, (10, 20), 16.0);
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	let first = grid.draw_with(&mut fonts, &format!("{KEYCAP}{KEYCAP}"));
	assert_eq!(fonts.clusters.len(), 1, "both cells share one entry");
	let again = grid.draw_with(&mut fonts, &format!("{KEYCAP}{KEYCAP}"));
	assert_eq!(first, again);
	assert_eq!(fonts.clusters.len(), 1);
}

#[test]
fn a_cluster_draws_at_the_grids_font_size() {
	// At 8 pixels per em the squares are 5 pixels wide.
	let grid = Grid::new(2, (10, 20), 8.0);
	let pixels = grid.draw(KEYCAP);
	let green = grid
		.cell_colors(&pixels, 0)
		.iter()
		.filter(|&&p| p == GREEN & 0xff_ffff)
		.count();
	assert_eq!(green, 25);
}
