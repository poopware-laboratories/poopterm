//! The status bar's model: where the shell is on the left; the grid size,
//! the theme and the last command's state on the right.
use crate::{
	render::{
		header::{Rect, Style},
		status_bar::StatusBar,
	},
	shell_integration::{Phase, ShellIntegration},
	theme::Theme,
	ui::header::TabPalette,
};

pub(crate) struct StatusInput<'a> {
	/// The link under the pointer, shown instead of the place.
	pub link: Option<&'a str>,
	/// Who or where the shell is, when that is not the local user.
	pub place: Option<&'a str>,
	/// The working directory's last component.
	pub cwd: &'a str,
	pub shell: &'a ShellIntegration,
	/// Columns and rows.
	pub size: (usize, usize),
	pub theme: &'a str,
}

/// The most characters of a running command line shown.
const COMMAND_CHARS: usize = 32;

/// Gaps between the right-hand items.
pub(crate) const SEPARATOR: &str = "  ·  ";

fn printable(text: &str) -> String {
	text.chars().filter(|ch| !ch.is_control()).collect()
}

/// The left and right texts.
pub(crate) fn status_text(input: &StatusInput) -> (String, String) {
	let left = match (input.link, input.place) {
		(Some(link), _) => format!("→ {}", printable(link)),
		(None, Some(place)) => format!("{}{SEPARATOR}{}", printable(place), printable(input.cwd)),
		(None, None) => printable(input.cwd),
	};
	let shell = input.shell;
	let command = match (shell.phase, &shell.command_line, &shell.last_error) {
		(Phase::Output, Some(line), _) => {
			let line = printable(line);
			let short: String = line.chars().take(COMMAND_CHARS).collect();
			let more = if short.len() < line.len() { "…" } else { "" };
			Some(format!("running {short}{more}"))
		}
		(Phase::Output, None, _) => Some("running".to_owned()),
		(_, _, Some(error)) => Some(format!("error {}", printable(error))),
		_ => shell.last_exit_code.map(|code| format!("exit {code}")),
	};
	let (cols, rows) = input.size;
	let mut right = format!("{cols} × {rows}{SEPARATOR}{}", printable(input.theme));
	if let Some(command) = command {
		right.push_str(SEPARATOR);
		right.push_str(&command);
	}
	(left, right)
}

/// The bar across `rect`, its text at `px`.
pub(crate) fn status_bar(
	input: &StatusInput,
	rect: Rect,
	(scale, px): (usize, f32),
	theme: &Theme,
) -> StatusBar {
	let palette = TabPalette::new(theme);
	let (left, right) = status_text(input);
	// A link reads as one, in the accent color.
	let left_color = if input.link.is_some() {
		palette.accent
	} else {
		theme.fg
	};
	StatusBar {
		rect,
		style: Style {
			scale,
			px,
			colors: palette.colors,
		},
		left: (left, left_color),
		right: (right, palette.dim),
	}
}
