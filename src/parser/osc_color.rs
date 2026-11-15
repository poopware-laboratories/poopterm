//! Color OSCs: xterm's 4, 5, 6, 10–19, 104–106 and 110–119, kitty's 21, and
//! the color stack (30001/30101). Replies end like the query did.
use super::color_spec::{encode, encode16, parse_color, parse_spec};
use crate::term::{Dynamic, Setting, Term};

/// Special colors (bold, underline, blink, reverse, italic) follow the
/// 256-color table in OSC 4 numbering.
const SPECIAL: usize = 5;

impl Term {
	/// Applies a color OSC; false when `command` is not one.
	pub(super) fn apply_osc_color(&mut self, command: &[u8], params: &[&[u8]], bell: bool) -> bool {
		let Some(number) = std::str::from_utf8(command)
			.ok()
			.and_then(|n| n.parse::<u32>().ok())
		else {
			return false;
		};
		match number {
			4 => self.osc_indexed(params, bell, false),
			5 => self.osc_indexed(params, bell, true),
			6 if params.get(1) == Some(&&b"bg"[..]) => self.osc_tab_color(params),
			6 | 106 => self.osc_special_modes(params),
			10..=19 => {
				for (offset, value) in params.iter().enumerate() {
					let Some(which) = Dynamic::from_index(number as usize - 10 + offset) else {
						break;
					};
					if *value == b"?" {
						let color = encode16(self.dynamic_color(which));
						let number = number as usize + offset;
						self.osc_reply(format!("{number};{color}"), bell);
					} else if let Some(color) = parse_color(value) {
						self.set_dynamic_color(which, Some(color));
					}
				}
			}
			104 => self.reset_indexed(params, false),
			105 => self.reset_indexed(params, true),
			110..=119 => {
				self.set_dynamic_color(Dynamic::from_index(number as usize - 110).unwrap(), None)
			}
			21 => self.osc_kitty_colors(params, bell),
			30001 => self.push_colors(0),
			30101 => self.pop_colors(0),
			_ => return false,
		}
		true
	}

	pub(super) fn osc_reply(&mut self, body: String, bell: bool) {
		let end = if bell { "\x07" } else { "\x1b\\" };
		self
			.responses
			.push(format!("\x1b]{body}{end}").into_bytes());
	}

	/// OSC 4 (`special` false) and OSC 5 pairs of a number and a color or `?`.
	fn osc_indexed(&mut self, params: &[&[u8]], bell: bool, special: bool) {
		for pair in params.as_chunks::<2>().0 {
			let Some(index) = number(pair[0]) else {
				continue;
			};
			// OSC 5 numbers count from the first special color.
			let slot = if special { index + 256 } else { index };
			if slot >= 256 + SPECIAL {
				continue;
			}
			if pair[1] == b"?" {
				let color = if slot < 256 {
					self.indexed_color(slot as u8)
				} else {
					self.colors.special[slot - 256].unwrap_or(self.default_fg)
				};
				let command = if special { 5 } else { 4 };
				self.osc_reply(format!("{command};{index};{}", encode16(color)), bell);
			} else if let Some(color) = parse_color(pair[1]) {
				if slot < 256 {
					self.color_overrides[slot] = Some(color);
				} else {
					self.colors.special[slot - 256] = Some(color);
				}
			}
		}
	}

	/// OSC 104 and 105: reset the listed colors, or all of them.
	fn reset_indexed(&mut self, params: &[&[u8]], special: bool) {
		if params.iter().all(|param| param.is_empty()) {
			if special {
				self.colors.special = [None; SPECIAL];
			} else {
				self.color_overrides.fill(None);
			}
			return;
		}
		for index in params.iter().filter_map(|param| number(param)) {
			match (special, index) {
				(false, 0..256) => self.color_overrides[index] = None,
				(false, 256..) | (true, _) => {
					let slot = if special { index } else { index - 256 };
					if let Some(color) = self.colors.special.get_mut(slot) {
						*color = None;
					}
				}
			}
		}
	}

	/// OSC 6 and 106: pairs of a special color number and an on/off flag.
	fn osc_special_modes(&mut self, params: &[&[u8]]) {
		let was_active = self.special_colors_active();
		for pair in params.as_chunks::<2>().0 {
			if let (Some(index), Some(flag)) = (number(pair[0]), number(pair[1]))
				&& let Some(mode) = self.colors.special_modes.get_mut(index)
			{
				*mode = flag != 0;
			}
		}
		if !was_active {
			self.sgr_fg = self.fg;
		}
		self.apply_special_colors();
	}

	/// kitty's OSC 21: `key=value` sets, `key=?` queries, `key=` makes a
	/// color dynamic and a bare `key` resets it.
	fn osc_kitty_colors(&mut self, params: &[&[u8]], bell: bool) {
		let mut reply = Vec::new();
		for field in params.iter().filter(|field| !field.is_empty()) {
			let (key, value) = match field.iter().position(|&byte| byte == b'=') {
				Some(at) => (&field[..at], Some(&field[at + 1..])),
				None => (*field, None),
			};
			let Some(target) = KittyKey::parse(key) else {
				let name = crate::graphics::base64::encode(&key[..key.len().min(256)], false);
				reply.push(format!("unknown={name}"));
				continue;
			};
			let key = String::from_utf8_lossy(key);
			match value {
				Some(b"?") => reply.push(format!("{key}={}", self.kitty_color(target))),
				Some(b"") => self.set_kitty_color(target, KittyValue::Dynamic),
				Some(value) => {
					if let Some(spec) = parse_spec(value) {
						self.set_kitty_color(target, KittyValue::Color(spec.rgb, spec.alpha));
					}
				}
				None => self.set_kitty_color(target, KittyValue::Reset),
			}
		}
		if !reply.is_empty() {
			self.osc_reply(format!("21;{}", reply.join(";")), bell);
		}
	}

	/// A kitty color as a reply gives it: empty when it follows the text.
	fn kitty_color(&self, target: KittyKey) -> String {
		let setting = |setting: Setting, which: Dynamic| match setting {
			Setting::Dynamic => String::new(),
			_ => encode(self.dynamic_color(which)),
		};
		let colors = &self.colors;
		match target {
			KittyKey::Palette(index) => encode(self.indexed_color(index)),
			KittyKey::Foreground => encode(self.default_fg),
			KittyKey::Background => encode(self.default_bg),
			KittyKey::SelectionBackground => {
				setting(colors.selection_background, Dynamic::HighlightBackground)
			}
			KittyKey::SelectionForeground => {
				setting(colors.selection_foreground, Dynamic::HighlightForeground)
			}
			// The cursor takes the cell's colors unless one is set.
			KittyKey::Cursor => match colors.cursor {
				Setting::Color(color) => encode(color),
				_ => String::new(),
			},
			KittyKey::CursorText => match colors.cursor_text {
				Setting::Color(color) => encode(color),
				_ => String::new(),
			},
			KittyKey::VisualBell => match colors.visual_bell {
				Setting::Color(color) => encode(color),
				_ => String::new(),
			},
			KittyKey::Transparent(index) => colors.transparent[index]
				.map(|(color, alpha)| format!("{}@{alpha}", encode(color)))
				.unwrap_or_default(),
		}
	}

	fn set_kitty_color(&mut self, target: KittyKey, value: KittyValue) {
		let color = match value {
			KittyValue::Color(color, _) => Some(color),
			_ => None,
		};
		let setting = match value {
			KittyValue::Reset => Setting::Default,
			KittyValue::Dynamic => Setting::Dynamic,
			KittyValue::Color(color, _) => Setting::Color(color),
		};
		let colors = &mut self.colors;
		match target {
			// The table and the default colors cannot be dynamic.
			KittyKey::Palette(index) if value != KittyValue::Dynamic => {
				self.color_overrides[index as usize] = color;
			}
			KittyKey::Foreground if value != KittyValue::Dynamic => {
				self.set_dynamic_color(Dynamic::Foreground, color);
			}
			KittyKey::Background if value != KittyValue::Dynamic => {
				self.set_dynamic_color(Dynamic::Background, color);
			}
			KittyKey::SelectionBackground => colors.selection_background = setting,
			KittyKey::SelectionForeground => colors.selection_foreground = setting,
			KittyKey::Cursor => colors.cursor = setting,
			KittyKey::CursorText => colors.cursor_text = setting,
			KittyKey::VisualBell => colors.visual_bell = setting,
			KittyKey::Transparent(index) => {
				colors.transparent[index] = match value {
					KittyValue::Color(color, alpha) => Some((color, alpha)),
					_ => None,
				};
			}
			_ => {}
		}
	}
}

#[derive(Clone, Copy, PartialEq)]
enum KittyValue {
	Reset,
	Dynamic,
	Color(u32, f32),
}

#[derive(Clone, Copy)]
enum KittyKey {
	Palette(u8),
	Foreground,
	Background,
	SelectionBackground,
	SelectionForeground,
	Cursor,
	CursorText,
	VisualBell,
	/// `transparent_background_color1..7`, from 0.
	Transparent(usize),
}

impl KittyKey {
	fn parse(key: &[u8]) -> Option<Self> {
		Some(match key {
			b"foreground" => Self::Foreground,
			b"background" => Self::Background,
			b"selection_background" => Self::SelectionBackground,
			b"selection_foreground" => Self::SelectionForeground,
			b"cursor" => Self::Cursor,
			b"cursor_text" => Self::CursorText,
			b"visual_bell" => Self::VisualBell,
			_ => {
				if let Some(digit) = key.strip_prefix(b"transparent_background_color")
					&& let [digit @ b'1'..=b'7'] = digit
				{
					return Some(Self::Transparent(usize::from(digit - b'1')));
				}
				Self::Palette(u8::try_from(number(key)?).ok()?)
			}
		})
	}
}

/// A decimal number of at most five digits.
fn number(text: &[u8]) -> Option<usize> {
	if text.is_empty() || text.len() > 5 || !text.iter().all(u8::is_ascii_digit) {
		return None;
	}
	std::str::from_utf8(text).ok()?.parse().ok()
}
