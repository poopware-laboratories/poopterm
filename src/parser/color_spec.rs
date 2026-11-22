//! Color specifications in OSC sequences: the XParseColor forms xterm and
//! kitty accept (`rgb:`, `rgbi:`, `#…` and X11 names) and the `rgb:` replies.

/// X11 color names, lowercase without spaces, sorted, with `rrggbb` values.
const NAMES: &str = include_str!("../../assets/x11-colors.txt");

/// A color and its alpha (1.0 unless a kitty `@alpha` suffix gives one).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spec {
	pub rgb: u32,
	pub alpha: f32,
}

/// Parses a color specification, with an optional `@alpha` suffix.
pub(crate) fn parse_spec(value: &[u8]) -> Option<Spec> {
	let (color, alpha) = match value.iter().rposition(|&byte| byte == b'@') {
		Some(at) => (&value[..at], Some(parse_float(&value[at + 1..])?)),
		None => (value, None),
	};
	Some(Spec {
		rgb: parse_color(color)?,
		alpha: alpha.map_or(1.0, |alpha| alpha.clamp(0.0, 1.0)),
	})
}

/// Parses a color specification without alpha.
pub(crate) fn parse_color(value: &[u8]) -> Option<u32> {
	let text = std::str::from_utf8(value).ok()?;
	if let Some(channels) = text.strip_prefix("rgb:") {
		return rgb(channels.split('/'), |part| {
			// One to four digits, checked before they scale anything.
			let value = hex(part)?;
			let max = (1u32 << (4 * part.len())) - 1;
			Some((value * 255 + max / 2) / max)
		});
	}
	if let Some(channels) = text.strip_prefix("rgbi:") {
		return rgb(channels.split('/'), |part| {
			Some((parse_float(part.as_bytes())?.clamp(0.0, 1.0) * 255.0).round() as u32)
		});
	}
	if let Some(digits) = text.strip_prefix('#') {
		if !matches!(digits.len(), 3 | 6 | 9 | 12) {
			return None;
		}
		let width = digits.len() / 3;
		let channels = (0..3).map(|i| &digits[i * width..][..width]);
		// Short forms are the most significant bits, not scaled values.
		return rgb(channels, |part| {
			let value = hex(part)?;
			Some(if width == 1 {
				value << 4
			} else {
				value >> (4 * (width - 2))
			})
		});
	}
	named(text)
}

/// Three channels, each converted to 0–255 by `channel`.
fn rgb<'a>(
	mut parts: impl Iterator<Item = &'a str>,
	channel: impl Fn(&str) -> Option<u32>,
) -> Option<u32> {
	let mut color = 0;
	for _ in 0..3 {
		color = color << 8 | channel(parts.next()?)?;
	}
	parts.next().is_none().then_some(color)
}

/// One to four hexadecimal digits.
fn hex(digits: &str) -> Option<u32> {
	if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
		return None;
	}
	u32::from_str_radix(digits, 16).ok()
}

/// A decimal number with an optional sign, point and exponent; no
/// `inf` or `nan`.
fn parse_float(text: &[u8]) -> Option<f32> {
	let valid = |byte: &u8| byte.is_ascii_digit() || b".eE+-".contains(byte);
	if text.is_empty() || text.len() > 32 || !text.iter().all(valid) {
		return None;
	}
	std::str::from_utf8(text).ok()?.parse().ok()
}

fn named(name: &str) -> Option<u32> {
	if name.len() > 32 {
		return None;
	}
	let key: String = name
		.chars()
		.filter(|&c| c != ' ')
		.map(|c| c.to_ascii_lowercase())
		.collect();
	let lines: Vec<&str> = NAMES
		.lines()
		.filter(|line| !line.starts_with('#'))
		.collect();
	let index = lines
		.binary_search_by(|line| line.split(' ').next().unwrap_or("").cmp(&key))
		.ok()?;
	u32::from_str_radix(lines[index].split(' ').nth(1)?, 16).ok()
}

/// `rgb:rrrr/gggg/bbbb`, xterm's reply to OSC 4, 5 and 10–19 queries: each
/// 8-bit channel repeated to 16 bits, as XQueryColor scales it.
pub(crate) fn encode16(color: u32) -> String {
	let [_, r, g, b] = color.to_be_bytes();
	format!("rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}")
}

/// `rgb:rr/gg/bb`, the form kitty's OSC 21 replies with.
pub(crate) fn encode(color: u32) -> String {
	format!(
		"rgb:{:02x}/{:02x}/{:02x}",
		color >> 16 & 0xff,
		color >> 8 & 0xff,
		color & 0xff
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rgb_scales_each_channel_by_its_digit_count() {
		assert_eq!(parse_color(b"rgb:f/0/8"), Some(0xff_00_88));
		assert_eq!(parse_color(b"rgb:12/ab/FF"), Some(0x12_ab_ff));
		assert_eq!(parse_color(b"rgb:800/fff/000"), Some(0x80_ff_00));
		assert_eq!(parse_color(b"rgb:ffff/8000/0000"), Some(0xff_80_00));
		for bad in [
			"rgb:1/2",
			"rgb:1/2/3/4",
			"rgb:12345/0/0",
			"rgb:g/0/0",
			"rgb://",
			// Long enough that scaling by the digit count before checking it
			// would shift past 32 bits.
			"rgb:12345678/0/0",
			"rgb:0/0/0000000000000000000000000000000000000000",
		] {
			assert_eq!(parse_color(bad.as_bytes()), None, "{bad}");
		}
	}

	#[test]
	fn hash_forms_keep_the_most_significant_bits() {
		assert_eq!(parse_color(b"#3a7"), Some(0x30_a0_70));
		assert_eq!(parse_color(b"#12ab3C"), Some(0x12_ab_3c));
		assert_eq!(parse_color(b"#123456789"), Some(0x12_45_78));
		assert_eq!(parse_color(b"#ffff00008000"), Some(0xff_00_80));
		for bad in ["#", "#12", "#1234", "#12345g"] {
			assert_eq!(parse_color(bad.as_bytes()), None, "{bad}");
		}
	}

	#[test]
	fn rgbi_takes_clipped_floats() {
		assert_eq!(parse_color(b"rgbi:1/0.5/0"), Some(0xff_80_00));
		assert_eq!(parse_color(b"rgbi:2/-1/1e-1"), Some(0xff_00_1a));
		assert_eq!(parse_color(b"rgbi:1/x/0"), None);
	}

	#[test]
	fn x11_names_ignore_case_and_spaces() {
		assert_eq!(parse_color(b"red"), Some(0xff0000));
		assert_eq!(parse_color(b"Light Goldenrod"), Some(0xeedd82));
		assert_eq!(parse_color(b"GRAY50"), Some(0x7f7f7f));
		assert_eq!(parse_color(b"no such color"), None);
		assert_eq!(parse_color(b""), None);
	}

	#[test]
	fn alpha_suffixes_are_clipped_and_optional() {
		let spec = |text: &str| parse_spec(text.as_bytes());
		assert_eq!(
			spec("red@0.5"),
			Some(Spec {
				rgb: 0xff0000,
				alpha: 0.5
			})
		);
		assert_eq!(
			spec("#00ff00@3"),
			Some(Spec {
				rgb: 0xff00,
				alpha: 1.0
			})
		);
		assert_eq!(
			spec("blue"),
			Some(Spec {
				rgb: 0xff,
				alpha: 1.0
			})
		);
		assert_eq!(spec("blue@"), None);
		// Alpha is a kitty extension; plain color parsing refuses it.
		assert_eq!(parse_color(b"red@0.5"), None);
	}

	#[test]
	fn kitty_replies_use_two_digit_rgb() {
		assert_eq!(encode(0x0a_ff_10), "rgb:0a/ff/10");
	}

	#[test]
	fn xterm_replies_repeat_each_byte_into_sixteen_bits() {
		assert_eq!(encode16(0x0a_ff_10), "rgb:0a0a/ffff/1010");
		assert_eq!(encode16(0), "rgb:0000/0000/0000");
		assert_eq!(encode16(0xff_ff_ff), "rgb:ffff/ffff/ffff");
		// What xterm answers after each form sets the color (esctest).
		for spec in [
			"rgb:f0f0/f0f0/f0f0",
			"#fff",
			"#f0f0f0",
			"#f00f00f00",
			"#f000f000f000",
		] {
			assert_eq!(
				encode16(parse_color(spec.as_bytes()).unwrap()),
				"rgb:f0f0/f0f0/f0f0"
			);
		}
		for spec in [
			"rgb:8080/8080/8080",
			"#888",
			"#808080",
			"#800800800",
			"#800080008000",
		] {
			assert_eq!(
				encode16(parse_color(spec.as_bytes()).unwrap()),
				"rgb:8080/8080/8080"
			);
		}
		// A reply read back sets the same color: queries round-trip.
		for color in [0x12_34_56, 0xfe_01_80, 0x00_ff_7f] {
			assert_eq!(parse_color(encode16(color).as_bytes()), Some(color));
		}
	}
}
