//! The key bar above a phone's on-screen keyboard (iOS), for the keys it
//! lacks: Esc, Tab, the arrows, a Ctrl that holds for the next key typed,
//! and the command palette, plus a key to hide it. Where the keys sit, which
//! one a tap is on, and what Ctrl does to the key after it.
use crate::{input::keys::Modifiers, render::scene::Rect};
use winit::keyboard::{Key, NamedKey};

/// The bar's height and the room around and between keys, in logical pixels.
pub(crate) const HEIGHT: usize = 44;
const GAP: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarKey {
	Esc,
	Ctrl,
	Tab,
	Left,
	Down,
	Up,
	Right,
	Palette,
	Hide,
}

/// The keys, left to right, with their labels.
pub(crate) const KEYS: [(BarKey, &str); 9] = [
	(BarKey::Esc, "esc"),
	(BarKey::Ctrl, "ctrl"),
	(BarKey::Tab, "tab"),
	(BarKey::Left, "←"),
	(BarKey::Down, "↓"),
	(BarKey::Up, "↑"),
	(BarKey::Right, "→"),
	(BarKey::Palette, "cmd"),
	(BarKey::Hide, "hide"),
];

/// What pressing a key does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Press {
	/// Send this key, with the modifiers given, as typing it would.
	Send(Key, Modifiers),
	/// Ctrl now holds, or no longer does.
	Latched(bool),
	/// Open the command palette.
	Palette,
	/// Dismiss the on-screen keyboard.
	HideKeyboard,
}

/// Each key's rectangle in a bar from `(x, y)`, `width` pixels wide, at
/// `scale`: equal widths sharing the room left by the gaps.
pub(crate) fn key_rects((x, y): (usize, usize), width: usize, scale: usize) -> Vec<(Rect, BarKey)> {
	let gap = GAP * scale;
	let count = KEYS.len();
	let room = width.saturating_sub(gap * (count + 1));
	let key_width = room / count;
	let height = (HEIGHT - 2 * GAP) * scale;
	KEYS
		.iter()
		.enumerate()
		.map(|(index, &(key, _))| {
			let left = x + gap + index * (key_width + gap);
			((left, y + gap, key_width, height), key)
		})
		.collect()
}

/// The key a tap at `point` is on, if any.
pub(crate) fn key_at(point: (usize, usize), rects: &[(Rect, BarKey)]) -> Option<BarKey> {
	let (x, y) = point;
	rects
		.iter()
		.find(|((rx, ry, rw, rh), _)| (*rx..rx + rw).contains(&x) && (*ry..ry + rh).contains(&y))
		.map(|&(_, key)| key)
}

/// The bar's state: whether Ctrl holds for the next key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyBar {
	ctrl: bool,
}

impl KeyBar {
	/// Whether Ctrl holds.
	pub(crate) fn ctrl(&self) -> bool {
		self.ctrl
	}

	/// What pressing `key` does; a key sent takes Ctrl with it if it held.
	pub(crate) fn press(&mut self, key: BarKey) -> Press {
		let named = match key {
			BarKey::Ctrl => {
				self.ctrl = !self.ctrl;
				return Press::Latched(self.ctrl);
			}
			BarKey::Palette => {
				self.ctrl = false;
				return Press::Palette;
			}
			BarKey::Hide => {
				self.ctrl = false;
				return Press::HideKeyboard;
			}
			BarKey::Esc => NamedKey::Escape,
			BarKey::Tab => NamedKey::Tab,
			BarKey::Left => NamedKey::ArrowLeft,
			BarKey::Down => NamedKey::ArrowDown,
			BarKey::Up => NamedKey::ArrowUp,
			BarKey::Right => NamedKey::ArrowRight,
		};
		let key = Key::Named(named);
		let mods = self.take(&key, Modifiers::default());
		Press::Send(key, mods)
	}

	/// The modifiers for `key` typed with `mods` held: Ctrl added if it
	/// held, which it then no longer does. A capital letter with Ctrl is
	/// Ctrl+Shift, as on a keyboard (Ctrl, then P: the palette).
	pub(crate) fn take(&mut self, key: &Key, mods: Modifiers) -> Modifiers {
		if !std::mem::take(&mut self.ctrl) {
			return mods;
		}
		let capital = matches!(key, Key::Character(text)
			if text.chars().count() == 1 && text.chars().all(|ch| ch.is_ascii_uppercase()));
		Modifiers {
			ctrl: true,
			shift: mods.shift || capital,
			..mods
		}
	}
}

/// Whether the bar shows: on iOS while the on-screen keyboard is up, its
/// top `covered` pixels above the home indicator's `home`.
pub(crate) fn shown(ios: bool, bottom: usize, home: usize) -> bool {
	ios && bottom > home
}

/// The bottom inset with the bar: `bottom` (the keyboard's and home
/// indicator's) plus the bar's height while it shows.
pub(crate) fn bottom_with_bar(ios: bool, bottom: usize, home: usize, scale: usize) -> usize {
	if shown(ios, bottom, home) {
		bottom + HEIGHT * scale
	} else {
		bottom
	}
}

/// The bar's strip in a window `(width, height)` large: right above the
/// keyboard's top, `bottom` pixels up, and clear of the `sides` insets.
pub(crate) fn strip(
	(width, height): (usize, usize),
	bottom: usize,
	(left, right): (usize, usize),
	scale: usize,
) -> Rect {
	let tall = HEIGHT * scale;
	let y = height.saturating_sub(bottom + tall);
	(
		left.min(width),
		y,
		width.saturating_sub(left + right),
		tall.min(height),
	)
}

#[cfg(test)]
#[path = "key_bar_tests.rs"]
mod tests;
