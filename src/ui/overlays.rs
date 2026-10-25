//! The search box and command palette: where they float, and what their
//! field and rows show.
use crate::{
	render::{
		header::{HeaderColors, Style},
		popover::{Field, Popover, PopoverRow},
	},
	theme::Theme,
	ui::header::TabPalette,
};

/// Commands offered by the command palette, in display order.
pub(crate) const PALETTE_COMMANDS: [&str; 7] = [
	"New tab",
	"Settings",
	"About poopterm",
	"Cycle theme",
	"Copy last command output",
	"Close tab",
	"Rename active tab",
];

/// The commands whose name holds `query`, ignoring case; Enter runs the
/// first.
pub(crate) fn palette_matches(query: &str) -> Vec<&'static str> {
	let query = query.to_lowercase();
	let matching = PALETTE_COMMANDS.into_iter();
	matching
		.filter(|command| command.to_lowercase().contains(&query))
		.collect()
}

/// Where an overlay floats: the window's size, the top of the terminal
/// area, and the scale.
#[derive(Clone, Copy)]
pub(crate) struct Place {
	pub window: (usize, usize),
	pub top: usize,
	pub scale: usize,
}

/// How far popovers keep from the window's edges, before scaling.
const MARGIN: usize = 12;
/// The room between a popover's edge and its field and rows.
const INSET: usize = 8;
const FIELD_HEIGHT: usize = 34;
const ROW_HEIGHT: usize = 32;

/// The header's colors, with dim text kept readable on every shade.
fn style(place: Place, px: f32, theme: &Theme) -> Style {
	let palette = TabPalette::new(theme);
	let colors = HeaderColors {
		dim: palette.dim,
		..palette.colors
	};
	Style {
		scale: place.scale,
		px,
		colors,
	}
}

/// The search box at the top right: the query, and how many matches it has
/// and which is current, once there is a query.
pub(crate) fn search_box<'a>(
	place: Place,
	(query, matches, current): (&'a str, usize, usize),
	px: f32,
	theme: &Theme,
) -> Popover<'a> {
	let Place { window, top, scale } = place;
	let width = (380 * scale).min(window.0.saturating_sub(2 * MARGIN * scale));
	let height = (FIELD_HEIGHT + 2 * INSET) * scale;
	let x = window.0.saturating_sub(width + MARGIN * scale);
	let y = top + INSET * scale;
	let trailing = match matches {
		_ if query.is_empty() => None,
		0 => Some("No matches".to_owned()),
		_ => Some(format!("{} of {matches}", current + 1)),
	};
	Popover {
		rect: (x, y, width, height),
		style: style(place, px, theme),
		field: Field {
			rect: inset((x, y, width), scale),
			text: query,
			placeholder: "Find",
			trailing,
		},
		rows: Vec::new(),
		note: None,
	}
}

/// The field across the top of a popover at `(x, y)`, `width` wide.
fn inset((x, y, width): (usize, usize, usize), scale: usize) -> (usize, usize, usize, usize) {
	let room = width.saturating_sub(2 * INSET * scale);
	(
		x + INSET * scale,
		y + INSET * scale,
		room,
		FIELD_HEIGHT * scale,
	)
}

/// The command palette in the middle at the top: the query, then the
/// matching commands with the first, which Enter runs, selected. While a
/// tab is renamed the field holds the name and a note says what keys do.
pub(crate) fn command_palette<'a>(
	place: Place,
	(query, renaming): (&'a str, bool),
	px: f32,
	theme: &Theme,
) -> Popover<'a> {
	let Place { window, top, scale } = place;
	let width = (520 * scale).min(window.0.saturating_sub(2 * MARGIN * scale));
	let x = window.0.saturating_sub(width) / 2;
	let y = top + 2 * MARGIN * scale;
	let commands = if renaming {
		Vec::new()
	} else {
		palette_matches(query)
	};
	let below = y + (INSET + FIELD_HEIGHT + INSET / 2) * scale;
	// Rows that would run past the window's bottom are left out.
	let fit = window.1.saturating_sub(below + INSET * scale) / (ROW_HEIGHT * scale);
	let lines = commands.len().max(1).min(fit.max(1));
	let row = |index: usize| {
		let room = width.saturating_sub(2 * INSET * scale);
		(
			x + INSET * scale,
			below + index * ROW_HEIGHT * scale,
			room,
			ROW_HEIGHT * scale,
		)
	};
	let rows = commands.iter().take(lines).enumerate();
	let rows = rows.map(|(index, &text)| PopoverRow {
		rect: row(index),
		text,
		selected: index == 0,
	});
	let note = match () {
		_ if renaming => Some("Enter renames the tab, Esc cancels"),
		_ if commands.is_empty() => Some("No matching commands"),
		_ => None,
	};
	let height = (below - y) + lines * ROW_HEIGHT * scale + INSET * scale;
	Popover {
		rect: (x, y, width, height),
		style: style(place, px, theme),
		field: Field {
			rect: inset((x, y, width), scale),
			text: query,
			placeholder: if renaming {
				"Tab name"
			} else {
				"Run a command"
			},
			trailing: None,
		},
		rows: rows.collect(),
		note: note.map(|note| (row(0), note)),
	}
}
