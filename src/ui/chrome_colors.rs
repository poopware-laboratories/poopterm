//! The chrome's colors, each derived from the theme's background and text
//! by one blend rule, so every palette recolors the chrome without
//! per-theme code.
use crate::{
	render::blend::blend,
	theme::Theme,
	ui::contrast::{TEXT, luminance, readable},
};

/// What the tab bar, sidebar, status bar and popovers are drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChromeColors {
	/// The tab bar, sidebar and status bar.
	pub chrome: u32,
	/// The active tab: the terminal's own background, so the two join.
	pub raised: u32,
	/// Lines between the chrome and the terminal, and between panes.
	pub hairline: u32,
	/// Under a hovered tab or button.
	pub hover: u32,
	/// Under a pressed button.
	pub pressed: u32,
	/// Secondary text: inactive tabs, hints, the status bar.
	pub muted: u32,
	/// Popovers: the search box and the command palette.
	pub surface: u32,
	/// A popover's outline.
	pub pop_line: u32,
}

/// `share` percent as a [`blend`] weight out of 255, rounded to the nearest.
pub(crate) fn percent(share: u32) -> u32 {
	(share.min(100) * 255 + 50) / 100
}

/// Whether `theme` puts light text on a dark background.
pub(crate) fn is_dark(theme: &Theme) -> bool {
	luminance(theme.fg) >= luminance(theme.bg)
}

/// The line between the chrome and the terminal, and between panes; cheap
/// enough to work out every frame.
pub(crate) fn hairline(theme: &Theme) -> u32 {
	let share = if is_dark(theme) { 12 } else { 13 };
	blend(theme.bg, theme.fg, percent(share))
}

/// Every chrome color of `theme`. Finding readable muted text takes a
/// search, so callers work this out once per frame at most.
pub(crate) fn chrome_colors(theme: &Theme) -> ChromeColors {
	let (bg, fg, dark) = (theme.bg, theme.fg, is_dark(theme));
	let toward = |from, to, share| blend(from, to, percent(share));
	let (chrome, surface) = if dark {
		(toward(bg, 0x000000, 22), toward(bg, fg, 7))
	} else {
		(toward(bg, fg, 5), toward(bg, 0xffffff, 75))
	};
	let hover = toward(chrome, fg, 8);
	let faded = toward(fg, bg, if dark { 38 } else { 40 });
	let backgrounds = [chrome, hover, bg, surface];
	ChromeColors {
		chrome,
		raised: bg,
		hairline: hairline(theme),
		hover,
		pressed: toward(chrome, fg, 14),
		muted: readable(faded, fg, &backgrounds, TEXT),
		surface,
		pop_line: toward(surface, fg, if dark { 14 } else { 12 }),
	}
}
