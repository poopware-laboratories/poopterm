//! The search box and command palette: where they float, and what their
//! field and rows show.
use crate::{
	input::shortcuts::{ShortcutAction, Shortcuts},
	render::{
		header::{HeaderColors, Style},
		popover::{Field, Popover, PopoverButton, PopoverRow},
	},
	theme::Theme,
	ui::{
		header::TabPalette,
		search_controls::{SearchControl, geometry},
	},
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

/// The shortcut action `command` has a key for, if any.
pub(crate) fn command_action(command: &str) -> Option<ShortcutAction> {
	match command {
		"New tab" => Some(ShortcutAction::NewTab),
		"Cycle theme" => Some(ShortcutAction::CycleTheme),
		"Close tab" => Some(ShortcutAction::CloseTab),
		_ => None,
	}
}

/// Each command's chord as the platform writes it, from the bindings in
/// use, for the palette to show beside the command.
pub(crate) fn shortcut_hints(
	shortcuts: &Shortcuts,
	mac: bool,
) -> impl Fn(&str) -> Option<String> + '_ {
	move |command| {
		let action = command_action(command)?;
		Some(shortcuts.get(action).display(mac))
	}
}

/// Whether running `command` keeps the palette open: renaming a tab edits
/// the name in the palette's field.
pub(crate) fn stays_open(command: &str) -> bool {
	command == "Rename active tab"
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
pub(crate) const MARGIN: usize = 12;
/// The room between a popover's edge and its field and rows.
pub(crate) const INSET: usize = 8;
pub(crate) const FIELD_HEIGHT: usize = 34;
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
/// and which is current, once there is a query. Its buttons are added by
/// [`search_buttons`].
pub(crate) fn search_box<'a>(
	place: Place,
	(query, matches, current): (&'a str, usize, usize),
	px: f32,
	theme: &Theme,
) -> Popover<'a> {
	let geometry = geometry(place);
	let trailing = match matches {
		_ if query.is_empty() => None,
		0 => Some("No matches".to_owned()),
		_ => Some(format!("{} of {matches}", current + 1)),
	};
	Popover {
		rect: geometry.panel,
		style: style(place, px, theme),
		field: Field {
			rect: geometry.field,
			text: query,
			placeholder: "Find",
			trailing,
		},
		rows: Vec::new(),
		note: None,
		buttons: Vec::new(),
	}
}

/// The search box's buttons, laid out as [`geometry`] places them: the case
/// button on when `case_sensitive`, and the one under `hover` lit.
pub(crate) fn search_buttons(
	place: Place,
	case_sensitive: bool,
	hover: Option<SearchControl>,
) -> Vec<PopoverButton<'static>> {
	let rects = geometry(place).buttons;
	rects
		.into_iter()
		.map(|(control, rect)| PopoverButton {
			rect,
			glyph: control.glyph(),
			hovered: hover == Some(control),
			active: control == SearchControl::Case && case_sensitive,
		})
		.collect()
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
		hint: None,
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
		buttons: Vec::new(),
	}
}

/// The row after stepping `delta` from `current` among `count` rows,
/// wrapping at both ends. A `current` past the rows counts as the first.
pub(crate) fn step_selection(current: usize, count: usize, delta: isize) -> usize {
	if count == 0 {
		return 0;
	}
	let at = if current < count { current } else { 0 };
	(at as isize + delta).rem_euclid(count as isize) as usize
}

/// The command Enter runs with row `index` selected among the matches of
/// `query`; an `index` past them takes the last.
pub(crate) fn selected_command(query: &str, index: usize) -> Option<&'static str> {
	let matches = palette_matches(query);
	matches
		.get(index.min(matches.len().saturating_sub(1)))
		.copied()
}

impl Popover<'_> {
	/// Marks row `index`, or the last row when it is past them, as the
	/// selected one and no other.
	pub(crate) fn select(&mut self, index: usize) {
		let last = self.rows.len().saturating_sub(1);
		for (at, row) in self.rows.iter_mut().enumerate() {
			row.selected = at == index.min(last);
		}
	}
}
