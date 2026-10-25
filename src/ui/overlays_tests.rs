//! Tests of the overlay models: the palette's matching, and where the
//! search box and command palette float and what they show.
use super::{
	header::{TabPalette, header_colors},
	overlays::{PALETTE_COMMANDS, Place, command_palette, palette_matches, search_box},
};
use crate::{
	render::{
		header::{HeaderColors, Rect},
		popover::Popover,
	},
	theme::{Theme, ThemeKind},
};

fn theme() -> Theme {
	Theme::from_kind(ThemeKind::MacOsDark)
}

fn place(window: (usize, usize), scale: usize) -> Place {
	Place {
		window,
		top: 44 * scale,
		scale,
	}
}

fn search(query: &str, matches: usize, current: usize) -> Popover<'_> {
	search_box(
		place((1000, 600), 1),
		(query, matches, current),
		14.0,
		&theme(),
	)
}

fn palette(query: &str, renaming: bool) -> Popover<'_> {
	command_palette(place((1000, 600), 1), (query, renaming), 14.0, &theme())
}

fn texts<'a>(popover: &Popover<'a>) -> Vec<&'a str> {
	popover.rows.iter().map(|row| row.text).collect()
}

#[test]
fn an_empty_query_matches_every_command_in_order() {
	assert_eq!(palette_matches(""), PALETTE_COMMANDS);
}

#[test]
fn commands_match_anywhere_in_their_name_ignoring_case() {
	assert_eq!(
		palette_matches("tab"),
		["New tab", "Close tab", "Rename active tab"]
	);
	assert_eq!(palette_matches("SETT"), ["Settings"]);
	assert_eq!(palette_matches("t p"), ["About poopterm"]);
	assert_eq!(palette_matches("zzz"), Vec::<&str>::new());
}

#[test]
fn matching_lowers_the_case_of_letters_beyond_ascii_too() {
	// No command holds these, but lowering must not panic or match wrongly.
	assert!(palette_matches("ÉCRAN").is_empty());
	assert_eq!(palette_matches("ABOUT POOPTERM"), ["About poopterm"]);
}

#[test]
fn the_search_box_floats_at_the_top_right_below_the_tabs() {
	let popover = search("poop", 8, 0);
	assert_eq!(popover.rect, (608, 52, 380, 50));
	assert_eq!(popover.field.rect, (616, 60, 364, 34));
	assert_eq!(popover.field.text, "poop");
	assert_eq!(popover.field.placeholder, "Find");
	assert!(popover.rows.is_empty() && popover.note.is_none());
}

#[test]
fn the_search_box_counts_matches_once_there_is_a_query() {
	assert_eq!(search("", 0, 0).field.trailing, None);
	assert_eq!(search("", 5, 2).field.trailing, None);
	assert_eq!(
		search("x", 0, 0).field.trailing.as_deref(),
		Some("No matches")
	);
	assert_eq!(search("x", 8, 0).field.trailing.as_deref(), Some("1 of 8"));
	assert_eq!(search("x", 8, 7).field.trailing.as_deref(), Some("8 of 8"));
}

#[test]
fn a_narrow_window_narrows_the_search_box_to_its_margins() {
	let popover = search_box(place((300, 600), 1), ("x", 1, 0), 14.0, &theme());
	assert_eq!(popover.rect, (12, 52, 276, 50));
	let tiny = search_box(place((10, 600), 1), ("x", 1, 0), 14.0, &theme());
	assert_eq!((tiny.rect.0, tiny.rect.2), (0, 0));
	assert_eq!(tiny.field.rect.2, 0);
}

#[test]
fn the_search_box_doubles_at_twice_the_scale() {
	let popover = search_box(place((2000, 1200), 2), ("x", 1, 0), 28.0, &theme());
	assert_eq!(popover.rect, (2000 - 760 - 24, 104, 760, 100));
	assert_eq!(popover.field.rect, (2000 - 760 - 24 + 16, 120, 728, 68));
	assert_eq!(popover.style.scale, 2);
}

#[test]
fn popovers_take_the_header_colors_with_readable_dim_text() {
	let theme = theme();
	for popover in [search("x", 1, 0), palette("", false)] {
		let colors = header_colors(&theme);
		let dim = TabPalette::new(&theme).dim;
		assert_eq!(popover.style.colors, HeaderColors { dim, ..colors });
		assert_eq!(popover.style.px, 14.0);
	}
}

#[test]
fn the_palette_floats_centered_under_the_tabs() {
	let popover = palette("", false);
	assert_eq!(
		(popover.rect.0, popover.rect.1, popover.rect.2),
		(240, 68, 520)
	);
	assert_eq!(popover.field.rect, (248, 76, 504, 34));
	assert_eq!(popover.field.placeholder, "Run a command");
	assert_eq!(popover.field.trailing, None);
}

#[test]
fn the_palette_lists_every_matching_command_in_rows_below_the_field() {
	let popover = palette("", false);
	assert_eq!(texts(&popover), PALETTE_COMMANDS);
	for (index, row) in popover.rows.iter().enumerate() {
		assert_eq!(row.rect, (248, 114 + 32 * index, 504, 32), "row {index}");
	}
	// The field, the rows, and the inset below them.
	assert_eq!(popover.rect.3, 46 + 7 * 32 + 8);
	assert!(popover.note.is_none());
}

#[test]
fn the_first_match_is_selected_as_enter_runs_it() {
	for query in ["", "tab", "o", "copy"] {
		let popover = palette(query, false);
		let selected: Vec<_> = popover.rows.iter().filter(|row| row.selected).collect();
		assert_eq!(selected.len(), 1, "{query:?}");
		assert_eq!(selected[0].text, palette_matches(query)[0], "{query:?}");
		assert!(popover.rows[0].selected, "{query:?}");
	}
}

#[test]
fn the_palette_shrinks_to_its_matches() {
	let popover = palette("tab", false);
	assert_eq!(
		texts(&popover),
		["New tab", "Close tab", "Rename active tab"]
	);
	assert_eq!(popover.rect.3, 46 + 3 * 32 + 8);
	assert_eq!(popover.field.text, "tab");
}

#[test]
fn without_matches_the_palette_says_so_in_a_row_of_its_own() {
	let popover = palette("zzz", false);
	assert!(popover.rows.is_empty());
	let (rect, note) = popover.note.unwrap();
	assert_eq!(note, "No matching commands");
	assert_eq!(rect, (248, 114, 504, 32));
	assert_eq!(popover.rect.3, 46 + 32 + 8);
}

#[test]
fn renaming_a_tab_shows_its_name_and_what_the_keys_do() {
	let popover = palette("my shell", true);
	assert!(popover.rows.is_empty(), "no commands while renaming");
	assert_eq!(popover.field.text, "my shell");
	assert_eq!(popover.field.placeholder, "Tab name");
	let (_, note) = popover.note.unwrap();
	assert_eq!(note, "Enter renames the tab, Esc cancels");
	// A name that matches commands lists none of them.
	assert!(palette("tab", true).rows.is_empty());
}

#[test]
fn a_short_window_lists_only_the_rows_that_fit() {
	// From 68 down: the field takes 46, then rows of 32 and an inset of 8.
	for (height, rows) in [
		(600, 7),
		(300, 5),
		(250, 4),
		(249, 3),
		(186, 2),
		(185, 1),
		(154, 1),
	] {
		let popover = command_palette(place((1000, height), 1), ("", false), 14.0, &theme());
		assert_eq!(popover.rows.len(), rows, "height {height}");
		assert!(popover.rect.1 + popover.rect.3 <= height, "height {height}");
	}
	// Always one line, however short.
	let tiny = command_palette(place((1000, 50), 1), ("", false), 14.0, &theme());
	assert_eq!(tiny.rows.len(), 1);
}

#[test]
fn the_palette_doubles_at_twice_the_scale() {
	let popover = command_palette(place((2000, 1200), 2), ("", false), 28.0, &theme());
	assert_eq!(popover.rect, (480, 136, 1040, 2 * (46 + 7 * 32 + 8)));
	assert_eq!(popover.rows[1].rect, (496, 136 + 92 + 64, 1008, 64));
}

/// Whether `inner` lies inside `outer`.
fn inside(inner: Rect, outer: Rect) -> bool {
	inner.0 >= outer.0
		&& inner.1 >= outer.1
		&& inner.0 + inner.2 <= outer.0 + outer.2
		&& inner.1 + inner.3 <= outer.1 + outer.3
}

#[test]
fn fields_and_rows_stay_inside_their_popover_in_order_at_any_size() {
	let theme = theme();
	for scale in [1, 2, 3] {
		for width in (60..1400).step_by(97) {
			for height in (220..1000).step_by(71) {
				let place = place((width * scale, height * scale), scale);
				let at = format!("{width}×{height} at {scale}x");
				let popovers = [
					search_box(place, ("x", 3, 1), 14.0, &theme),
					command_palette(place, ("", false), 14.0, &theme),
					command_palette(place, ("zzz", false), 14.0, &theme),
					command_palette(place, ("name", true), 14.0, &theme),
				];
				for popover in popovers {
					let rect = popover.rect;
					assert!(rect.0 + rect.2 <= width * scale, "{at}: {rect:?}");
					assert!(rect.1 + rect.3 <= height * scale, "{at}: {rect:?}");
					assert!(inside(popover.field.rect, rect), "{at}");
					let mut bottom = popover.field.rect.1 + popover.field.rect.3;
					let lines = popover.rows.iter().map(|row| row.rect);
					for line in lines.chain(popover.note.map(|(rect, _)| rect)) {
						assert!(inside(line, rect), "{at}: {line:?} in {rect:?}");
						assert!(line.1 >= bottom, "{at}: lines overlap");
						bottom = line.1 + line.3;
					}
				}
			}
		}
	}
}

fn every_theme() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

#[test]
fn every_theme_keeps_popover_text_readable_where_it_is_drawn() {
	use super::contrast::{TEXT, ratio};
	for theme in every_theme() {
		let place = place((1000, 600), 1);
		let colors = command_palette(place, ("", false), 14.0, &theme)
			.style
			.colors;
		let name = theme.kind.name();
		// The query and caret on the field, rows on the panel and the
		// selected row; the placeholder, count and note are dim.
		let drawn = [
			("query", colors.fg, colors.hover),
			("row", colors.fg, colors.bar),
			("selected row", colors.fg, colors.raised),
			("placeholder", colors.dim, colors.hover),
			("count", colors.dim, colors.hover),
			("note", colors.dim, colors.bar),
		];
		for (what, color, under) in drawn {
			let contrast = ratio(color, under);
			assert!(
				contrast >= TEXT,
				"{name}: {what} {color:06x} on {under:06x} is {contrast:.2}"
			);
		}
	}
}

#[test]
fn spaces_in_a_query_match_like_any_other_character() {
	assert_eq!(palette_matches("w t"), ["New tab"]);
	assert_eq!(
		palette_matches(" tab"),
		["New tab", "Close tab", "Rename active tab"]
	);
	assert!(palette_matches("tab ").is_empty());
	assert!(palette_matches(" ").len() == 6, "all but Settings");
}

#[test]
fn typing_a_whole_command_name_selects_that_command() {
	for command in PALETTE_COMMANDS {
		for query in [command.to_owned(), command.to_uppercase()] {
			let popover = palette(&query, false);
			assert_eq!(texts(&popover), [command], "{query:?}");
			assert!(popover.rows[0].selected, "{query:?}");
		}
	}
}

#[test]
fn a_query_longer_than_every_command_matches_none() {
	let long = "New tab".repeat(10);
	assert!(palette_matches(&long).is_empty());
	assert_eq!(palette(&long, false).field.text, long);
}

#[test]
fn both_popovers_float_a_fixed_distance_below_the_top_of_the_terminal() {
	let theme = theme();
	for top in [0, 30, 44, 100] {
		let place = Place {
			window: (1000, 600),
			top,
			scale: 1,
		};
		let search = search_box(place, ("x", 1, 0), 14.0, &theme);
		let palette = command_palette(place, ("", false), 14.0, &theme);
		assert_eq!(search.rect.1, top + 8, "top {top}");
		assert_eq!(palette.rect.1, top + 24, "top {top}");
		assert_eq!(palette.rows[0].rect.1, top + 24 + 46, "top {top}");
	}
}

#[test]
fn the_palette_triples_at_three_times_the_scale() {
	let popover = command_palette(place((3000, 1800), 3), ("tab", false), 42.0, &theme());
	assert_eq!(popover.rect, (720, 132 + 72, 1560, 3 * (46 + 3 * 32 + 8)));
	assert_eq!(popover.field.rect, (744, 228, 1512, 102));
	assert_eq!(popover.rows[2].rect, (744, 204 + 138 + 192, 1512, 96));
}

#[test]
fn rows_and_notes_line_up_with_the_field() {
	for (query, renaming) in [("", false), ("zzz", false), ("x", true)] {
		let popover = palette(query, renaming);
		let field = popover.field.rect;
		let lines = popover.rows.iter().map(|row| row.rect);
		for line in lines.chain(popover.note.map(|(rect, _)| rect)) {
			assert_eq!((line.0, line.2), (field.0, field.2), "{query:?}");
		}
	}
}

#[test]
fn renaming_with_an_empty_name_shows_the_placeholder_and_the_note() {
	let popover = palette("", true);
	assert_eq!(popover.field.text, "");
	assert_eq!(popover.field.placeholder, "Tab name");
	let (rect, note) = popover.note.unwrap();
	assert_eq!(note, "Enter renames the tab, Esc cancels");
	assert_eq!(rect, popover_row(0));
	assert_eq!(popover.rect.3, 46 + 32 + 8, "one line for the note");
}

fn popover_row(index: usize) -> Rect {
	(248, 114 + 32 * index, 504, 32)
}
