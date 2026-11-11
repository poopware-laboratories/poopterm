//! What the pointer is over, read from one view of a terminal: the link
//! target under it and the shape the program asked for.
use super::hover::{GridPlace, Hover, hover};
use crate::{
	term::Term,
	theme::{Theme, ThemeKind},
};

/// A terminal of `cols` by `rows` that has been sent `bytes`.
fn terminal(cols: usize, rows: usize, bytes: &[u8]) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	crate::parser::Parser::new().advance(&mut term, bytes);
	term
}

/// A grid at (10, 20) with 8×16 cells, its area ending 4 pixels past the
/// last row of a 3-row terminal.
const PLACE: GridPlace = GridPlace {
	left: 10,
	top: 20,
	bottom: 20 + 3 * 16 + 4,
	cell: (8, 16),
};

/// The centre of cell (`row`, `col`) on screen.
fn at(row: usize, col: usize) -> (f64, f64) {
	((10 + col * 8 + 4) as f64, (20 + row * 16 + 8) as f64)
}

const LINKED: &[u8] = b"ab\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\cd";

#[test]
fn the_link_under_the_pointer_is_its_target() {
	let term = terminal(10, 3, LINKED);
	for col in 2..6 {
		let found = hover(&term, PLACE, at(0, col));
		assert_eq!(
			found.link.as_deref(),
			Some("https://example.com"),
			"col {col}"
		);
	}
	for col in [0, 1, 6, 7, 9] {
		assert_eq!(hover(&term, PLACE, at(0, col)).link, None, "col {col}");
	}
	assert_eq!(hover(&term, PLACE, at(1, 3)).link, None);
}

#[test]
fn nothing_is_linked_outside_the_grid() {
	let term = terminal(10, 3, LINKED);
	let right = (10 + 10 * 8) as f64;
	for pointer in [
		(9.9, 28.0),
		(right, 28.0),
		(40.0, 19.9),
		(40.0, 72.0),
		(-1.0, -1.0),
	] {
		assert_eq!(hover(&term, PLACE, pointer).link, None, "{pointer:?}");
	}
	// The pixels below the last row, inside the area, belong to that row.
	let term = terminal(10, 3, &[b"\r\n\r\n", LINKED].concat());
	assert_eq!(
		hover(&term, PLACE, (40.0, 71.0)).link.as_deref(),
		Some("https://example.com")
	);
}

#[test]
fn a_link_scrolled_into_history_is_found_where_it_is_shown() {
	let mut term = terminal(10, 3, &[LINKED, b"\r\n1\r\n2\r\n3"].concat());
	assert_eq!(term.scrollback.len(), 1);
	assert_eq!(
		hover(&term, PLACE, at(0, 3)).link,
		None,
		"row 0 shows line 1 now"
	);
	term.scroll_offset = 1;
	assert_eq!(
		hover(&term, PLACE, at(0, 3)).link.as_deref(),
		Some("https://example.com")
	);
	// An offset past the history shows its start, never a row before it.
	term.scroll_offset = 9;
	assert_eq!(
		hover(&term, PLACE, at(0, 3)).link.as_deref(),
		Some("https://example.com")
	);
}

#[test]
fn the_shape_the_program_asked_for_comes_with_the_hover_anywhere() {
	let term = terminal(10, 3, LINKED);
	assert_eq!(hover(&term, PLACE, at(0, 0)).shape, None);
	let term = terminal(10, 3, &[LINKED, b"\x1b]22;crosshair\x1b\\"].concat());
	for pointer in [at(0, 0), at(0, 3), (0.0, 0.0)] {
		let found = hover(&term, PLACE, pointer);
		assert_eq!(found.shape, Some("crosshair"), "{pointer:?}");
	}
	assert_eq!(
		hover(&term, PLACE, at(0, 3)),
		Hover {
			link: Some("https://example.com".into()),
			shape: Some("crosshair"),
		}
	);
}

#[test]
fn the_geometry_takes_its_size_and_first_row_from_the_terminal() {
	let mut term = terminal(7, 3, b"1\r\n2\r\n3\r\n4\r\n5");
	let geometry = PLACE.geometry(&term);
	assert_eq!((geometry.left, geometry.top, geometry.bottom), (10, 20, 72));
	assert_eq!(
		(geometry.cell, geometry.cols, geometry.rows),
		((8, 16), 7, 3)
	);
	assert_eq!(geometry.first_row, 2, "two lines in history, none scrolled");
	term.scroll_offset = 1;
	assert_eq!(PLACE.geometry(&term).first_row, 1);
	term.scroll_offset = 5;
	assert_eq!(PLACE.geometry(&term).first_row, 0);
}

#[test]
fn an_empty_or_cellless_grid_has_no_link() {
	let term = terminal(10, 3, LINKED);
	let flat = GridPlace {
		bottom: 20,
		..PLACE
	};
	assert_eq!(hover(&term, flat, at(0, 3)).link, None);
	let squashed = GridPlace {
		cell: (0, 0),
		..PLACE
	};
	// Zero-sized cells put every pixel past the grid's right edge.
	assert_eq!(hover(&term, squashed, at(0, 3)).link, None);
}

/// The lookup this module replaced, written out from its parts: the cell
/// under the pointer, then that cell's link.
fn reference(term: &Term, place: GridPlace, (x, y): (f64, f64)) -> Option<String> {
	let point = place.geometry(term).point_at(x, y)?;
	let cell = term.row_cells(point.row)?.get(point.col)?;
	term.hyperlink_uri(cell).map(str::to_owned)
}

#[test]
fn every_pixel_finds_what_the_cell_lookup_finds() {
	// Two links, one wrapped onto the next row, history, and a scroll.
	let text = [
		&b"one\r\n"[..],
		b"\x1b]8;;https://a\x1b\\aaaaaaaaaaaa\x1b]8;;\x1b\\ x ",
		b"\x1b]8;id=b;https://b\x1b\\bb\x1b]8;;\x1b\\\r\nlast",
	]
	.concat();
	let mut term = terminal(10, 3, &text);
	for offset in [0, 1, 2] {
		term.scroll_offset = offset;
		let mut found = 0;
		for y in (0..80).map(|y| y as f64 + 0.5) {
			for x in (0..100).map(|x| x as f64 + 0.5) {
				let link = hover(&term, PLACE, (x, y)).link;
				assert_eq!(
					link,
					reference(&term, PLACE, (x, y)),
					"({x}, {y}) offset {offset}"
				);
				found += usize::from(link.is_some());
			}
		}
		assert!(found > 0, "offset {offset} shows a link");
	}
}

#[test]
fn a_link_wrapped_onto_the_next_row_is_found_on_both() {
	let term = terminal(4, 3, b"\x1b]8;;https://wrap\x1b\\abcdef\x1b]8;;\x1b\\");
	for (row, col) in [(0, 0), (0, 3), (1, 0), (1, 1)] {
		let found = hover(&term, PLACE, at(row, col)).link;
		assert_eq!(found.as_deref(), Some("https://wrap"), "({row}, {col})");
	}
	assert_eq!(hover(&term, PLACE, at(1, 2)).link, None);
}

#[test]
fn both_halves_of_a_wide_linked_character_are_the_link() {
	let term = terminal(
		6,
		2,
		"\x1b]8;;https://wide\x1b\\日\x1b]8;;\x1b\\x".as_bytes(),
	);
	assert_eq!(term.grid[0][1].width, 0, "column 1 is the right half");
	for col in [0, 1] {
		let found = hover(&term, PLACE, at(0, col)).link;
		assert_eq!(found.as_deref(), Some("https://wide"), "col {col}");
	}
	assert_eq!(hover(&term, PLACE, at(0, 2)).link, None);
}

#[test]
fn each_screen_has_its_own_shape_and_its_own_links() {
	let mut term = terminal(10, 3, &[LINKED, b"\x1b]22;text\x1b\\"].concat());
	crate::parser::Parser::new().advance(&mut term, b"\x1b[?1049h\x1b]22;grab\x1b\\");
	let found = hover(&term, PLACE, at(0, 3));
	assert_eq!((found.link, found.shape), (None, Some("grab")));
	crate::parser::Parser::new().advance(&mut term, b"\x1b[?1049l");
	let found = hover(&term, PLACE, at(0, 3));
	assert_eq!(found.link.as_deref(), Some("https://example.com"));
	assert_eq!(found.shape, Some("text"));
}

#[test]
fn a_resized_terminal_is_hovered_at_its_new_size() {
	let mut term = terminal(10, 3, LINKED);
	term.resize(4, 3);
	assert_eq!(PLACE.geometry(&term).cols, 4);
	// Column 5 is past the narrower grid, whatever the row holds.
	assert_eq!(hover(&term, PLACE, at(0, 5)).link, None);
	let expected = Hover {
		link: reference(&term, PLACE, at(0, 3)),
		shape: None,
	};
	assert_eq!(hover(&term, PLACE, at(0, 3)), expected);
}

#[test]
fn rows_past_the_bottom_of_a_short_area_are_out_of_reach() {
	// An area one and a half rows tall shows the first row and half the next.
	let short = GridPlace {
		bottom: 20 + 24,
		..PLACE
	};
	let term = terminal(10, 3, &[b"\r\n", LINKED, b"\r\n", LINKED].concat());
	assert_eq!(hover(&term, short, at(0, 3)).link, None);
	let link = hover(&term, short, (40.0, 20.0 + 23.0)).link;
	assert_eq!(link.as_deref(), Some("https://example.com"));
	assert_eq!(hover(&term, short, at(2, 3)).link, None, "row 2 is cut off");
}

#[test]
fn runs_joined_by_one_link_id_all_lead_to_its_target() {
	let term = terminal(
		12,
		2,
		b"\x1b]8;id=x;https://same\x1b\\ab\x1b]8;;\x1b\\ - \x1b]8;id=x;https://same\x1b\\cd",
	);
	for col in [0, 1, 5, 6] {
		let found = hover(&term, PLACE, at(0, col)).link;
		assert_eq!(found.as_deref(), Some("https://same"), "col {col}");
	}
	for col in [2, 3, 4, 7] {
		assert_eq!(hover(&term, PLACE, at(0, col)).link, None, "col {col}");
	}
}
