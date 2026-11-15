//! Answers to device control string requests: DECRQSS reports a setting
//! as the control sequence that would set it, XTGETTCAP a capability from
//! poopterm's terminfo entry.
use crate::{
	term::{
		CursorShape, Term,
		attr::*,
		dcs::{Kind, Request},
	},
	terminfo_caps::{Capability, capability},
};

impl Term {
	/// Answers a finished request.
	pub(super) fn answer_dcs(&mut self, request: Request) {
		let data = if request.overflow {
			None
		} else {
			Some(&request.data[..])
		};
		let replies = match request.kind {
			Kind::Status => vec![match data.and_then(|setting| self.setting(setting)) {
				Some(setting) => format!("\x1bP1$r{setting}\x1b\\"),
				None => "\x1bP0$r\x1b\\".to_owned(),
			}],
			Kind::Capabilities => capability_replies(data.unwrap_or(b"")),
		};
		self
			.responses
			.extend(replies.into_iter().map(String::into_bytes));
	}

	/// The control sequence (without `CSI`) that sets `setting`.
	fn setting(&self, setting: &[u8]) -> Option<String> {
		Some(match setting {
			b"m" => format!("{}m", self.rendition()),
			b"r" => format!("{};{}r", self.scroll_top + 1, self.scroll_bottom + 1),
			b"s" => format!("{};{}s", self.left_margin + 1, self.right_margin + 1),
			// DECSCUSR's style: the steady one, or the blinking one before it.
			b" q" => {
				let steady = match self.cursor_shape {
					CursorShape::Block => 2,
					CursorShape::Underline => 4,
					CursorShape::Bar => 6,
				};
				format!("{} q", steady - usize::from(self.cursor_blink))
			}
			// A VT220 (as primary DA says) with 7-bit controls.
			b"\"p" => "62;1\"p".to_owned(),
			// No characters are protected.
			b"\"q" => "0\"q".to_owned(),
			b"t" => format!("{}t", self.rows),
			b"$|" => format!("{}$|", self.cols),
			b"*|" => format!("{}*|", self.rows),
			_ => return None,
		})
	}

	/// The current SGR parameters, starting from a reset. Colors equal to
	/// their palette entry are given by number, others as RGB.
	fn rendition(&self) -> String {
		let mut parts = vec!["0".to_owned()];
		let flags = [
			(self.bold, "1"),
			(self.attrs & DIM != 0, "2"),
			(self.attrs & ITALIC != 0, "3"),
			(
				self.attrs & UNDERLINE != 0 && self.attrs & DOUBLE_UNDERLINE == 0,
				"4",
			),
			(self.attrs & BLINK != 0, "5"),
			(self.reverse, "7"),
			(self.attrs & CONCEAL != 0, "8"),
			(self.attrs & STRIKE != 0, "9"),
			(self.attrs & DOUBLE_UNDERLINE != 0, "21"),
			(self.attrs & OVERLINE != 0, "53"),
		];
		parts.extend(
			flags
				.iter()
				.filter(|(on, _)| *on)
				.map(|(_, code)| (*code).to_owned()),
		);
		let fg = if self.special_colors_active() {
			self.sgr_fg
		} else {
			self.fg
		};
		if fg != self.default_fg {
			let index = u8::try_from(self.fg_id)
				.ok()
				.filter(|&index| self.indexed_color(index) == fg);
			parts.push(match index {
				Some(index @ 0..8) => format!("{}", 30 + index),
				Some(index @ 8..16) => format!("{}", 90 + index - 8),
				Some(index) => format!("38:5:{index}"),
				None => format!("38:2::{}", rgb(fg)),
			});
		}
		if self.bg != self.default_bg {
			let index = (0..16).find(|&index| self.indexed_color(index) == self.bg);
			parts.push(match index {
				Some(index @ 0..8) => format!("{}", 40 + index),
				Some(index) => format!("{}", 100 + index - 8),
				None => format!("48:2::{}", rgb(self.bg)),
			});
		}
		if let Some(color) = self.underline_color {
			parts.push(format!("58:2::{}", rgb(color)));
		}
		parts.join(";")
	}
}

/// XTGETTCAP: the hex-encoded names and values found, in one reply, then
/// an invalid reply if a name was not; it ends the list.
fn capability_replies(names: &[u8]) -> Vec<String> {
	let mut found = Vec::new();
	let mut unknown = false;
	for hex_name in names.split(|&byte| byte == b';') {
		let name = from_hex(hex_name).and_then(|name| String::from_utf8(name).ok());
		let Some((name, capability)) =
			name.and_then(|name| capability(&name).map(|capability| (name, capability)))
		else {
			unknown = true;
			break;
		};
		let mut pair = to_hex(name.as_bytes());
		match capability {
			Capability::Flag => {}
			Capability::Number(number) => {
				pair.push('=');
				pair.push_str(&to_hex(number.to_string().as_bytes()));
			}
			Capability::Text(text) => {
				pair.push('=');
				pair.push_str(&to_hex(&text));
			}
		}
		found.push(pair);
	}
	let mut replies = Vec::new();
	if !found.is_empty() {
		replies.push(format!("\x1bP1+r{}\x1b\\", found.join(";")));
	}
	if unknown {
		replies.push("\x1bP0+r\x1b\\".to_owned());
	}
	replies
}

fn from_hex(hex: &[u8]) -> Option<Vec<u8>> {
	let (pairs, rest) = hex.as_chunks::<2>();
	if !rest.is_empty() {
		return None;
	}
	pairs
		.iter()
		.map(|&[high, low]| {
			let digit = |byte: u8| char::from(byte).to_digit(16);
			Some((digit(high)? * 16 + digit(low)?) as u8)
		})
		.collect()
}

fn to_hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// `r:g:b` in decimal.
fn rgb(color: u32) -> String {
	format!(
		"{}:{}:{}",
		color >> 16 & 0xff,
		color >> 8 & 0xff,
		color & 0xff
	)
}
