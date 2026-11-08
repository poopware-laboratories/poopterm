//! Themes that publish all 16 ANSI colors: Dracula and Alucard, and Gruvbox
//! dark and light. Values come from each theme's published
//! palette; [`crate::theme_credits`] names the authors and licenses.
use crate::theme::{Theme, ThemeKind};

/// The built-in theme `kind`, for the kinds this file defines.
pub(crate) fn theme(kind: ThemeKind) -> Theme {
	let (fg, bg, border, palette, bright, selection) = match kind {
		// draculatheme.com/spec, "Dracula Classic" and its ANSI table.
		ThemeKind::Dracula => (
			0xF8F8F2,
			0x282A36,
			0x44475A,
			[
				0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C, 0xBD93F9, 0xFF79C6, 0x8BE9FD, 0xF8F8F2,
			],
			[
				0x6272A4, 0xFF6E6E, 0x69FF94, 0xFFFFA5, 0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF,
			],
			Some((0xF8F8F2, 0x44475A)),
		),
		// The same spec's light "Alucard Classic".
		ThemeKind::Alucard => (
			0x1F1F1F,
			0xFFFBEB,
			0xCFCFDE,
			[
				0xFFFBEB, 0xCB3A2A, 0x14710A, 0x846E15, 0x644AC9, 0xA3144D, 0x036A96, 0x1F1F1F,
			],
			[
				0x6C664B, 0xD74C3D, 0x198D0C, 0x9E841A, 0x7862D0, 0xBF185A, 0x047FB4, 0x2C2B31,
			],
			Some((0x1F1F1F, 0xCFCFDE)),
		),
		// Gruvbox's dark palette, medium contrast: bg0 on fg1, neutral
		// colors, then gray and the bright ones.
		ThemeKind::GruvboxDark => (
			0xEBDBB2,
			0x282828,
			0x3C3836,
			[
				0x282828, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0xA89984,
			],
			[
				0x928374, 0xFB4934, 0xB8BB26, 0xFABD2F, 0x83A598, 0xD3869B, 0x8EC07C, 0xEBDBB2,
			],
			None,
		),
		// Gruvbox's light palette: neutral colors, then gray and the faded ones.
		_ => (
			0x3C3836,
			0xFBF1C7,
			0xD5C4A1,
			[
				0xFBF1C7, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0x7C6F64,
			],
			[
				0x928374, 0x9D0006, 0x79740E, 0xB57614, 0x076678, 0x8F3F71, 0x427B58, 0x3C3836,
			],
			None,
		),
	};
	Theme {
		kind,
		fg,
		bg,
		border,
		palette,
		bright: Some(bright),
		cursor: None,
		selection,
	}
}
