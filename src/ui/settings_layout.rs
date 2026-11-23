//! Where the Settings main page's rows sit, in logical pixels, worked out
//! from the gaps between them instead of picked one by one, so the window
//! and the clicks on it agree.

pub(crate) const CONTROL: usize = 40;
pub(crate) const LABEL: usize = 18;
pub(crate) const CREDIT: usize = 14;
pub(crate) const WALLPAPER_BUTTONS: usize = 32;
pub(crate) const GAP_LABEL: usize = 8;
pub(crate) const GAP_BLOCK: usize = 16;
pub(crate) const GAP_HEADING: usize = 12;
/// Between a picker and the credit line under it.
pub(crate) const GAP_CREDIT: usize = 6;
pub(crate) const GAP_ROW: usize = 8;
pub(crate) const GAP_GROUP: usize = 8;
pub(crate) const SECTION_TOP: usize = 62;
pub(crate) const FOOTER: usize = 52;
pub(crate) const MIN_HEIGHT: usize = 600;

pub(crate) const MAIN: MainRows = main_rows();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MainRows {
	pub heading: usize,
	pub theme_label: usize,
	pub theme: usize,
	pub credit: usize,
	pub wallpaper_label: usize,
	pub wallpaper: usize,
	pub strength: usize,
	pub opacity: usize,
	pub font: usize,
	pub scrollback: usize,
	pub restore: usize,
}

/// The rows in the order they are drawn: each starts a gap below the end of
/// the one before it, a label a small gap above its control, and the
/// steppers one stride apart.
pub(crate) const fn main_rows() -> MainRows {
	let heading = SECTION_TOP;
	let theme_label = heading + LABEL + GAP_HEADING;
	let theme = theme_label + LABEL + GAP_LABEL;
	let credit = theme + CONTROL + GAP_CREDIT;
	let wallpaper_label = credit + CREDIT + GAP_BLOCK;
	let wallpaper = wallpaper_label + LABEL + GAP_LABEL;
	let strength = wallpaper + WALLPAPER_BUTTONS + GAP_BLOCK;
	let opacity = strength + CONTROL + GAP_ROW;
	let font = opacity + CONTROL + GAP_ROW + GAP_GROUP;
	let scrollback = font + CONTROL + GAP_ROW;
	let restore = scrollback + CONTROL + GAP_ROW;
	MainRows {
		heading,
		theme_label,
		theme,
		credit,
		wallpaper_label,
		wallpaper,
		strength,
		opacity,
		font,
		scrollback,
		restore,
	}
}

/// Pages narrower than this, in logical pixels (a phone's), keep the
/// controls at the right of their rows closer to the edge.
pub(crate) const NARROW: usize = 560;

/// How far from the right edge of a page `width` logical pixels wide the
/// controls at the right of a row (steppers, shortcut buttons) start, in
/// logical pixels: closer on a narrow page, so its labels keep their room.
pub(crate) const fn controls(width: usize) -> usize {
	if width >= NARROW { 240 } else { 176 }
}
