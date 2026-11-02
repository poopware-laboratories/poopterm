//! The Diagnostics page's colors, derived from the theme: muted secondary
//! text, a color per drawing status, and the shades of rules and the log's
//! well. Every text color stays readable on the page and in the well.
use super::contrast::{DIM, TEXT, readable};
use crate::{render::blend::blend, theme::Theme};

/// Colors the Diagnostics page draws with, beside the theme's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticsColors {
	/// Headings, details, times and footnotes.
	pub(crate) muted: u32,
	/// A feature the GPU draws in full.
	pub(crate) accelerated: u32,
	/// A feature drawn by Vulkan on a CPU device.
	pub(crate) limited: u32,
	/// A feature software draws.
	pub(crate) software: u32,
	/// The line under each row of a table.
	pub(crate) rule: u32,
	/// The log's background.
	pub(crate) well: u32,
}

/// How far from the page toward its border the well and the rules sit,
/// out of 255.
const WELL: u32 = 64;
const RULE: u32 = 128;

/// The Diagnostics page's colors for `theme`.
pub(crate) fn diagnostics_colors(theme: &Theme) -> DiagnosticsColors {
	let (well, rule) = (
		blend(theme.bg, theme.border, WELL),
		blend(theme.bg, theme.border, RULE),
	);
	let readable = |color| readable(color, theme.fg, &[theme.bg, well], TEXT);
	let muted = readable(blend(theme.bg, theme.fg, DIM));
	DiagnosticsColors {
		muted,
		accelerated: readable(theme.palette[2]),
		limited: readable(theme.palette[3]),
		software: muted,
		rule,
		well,
	}
}
