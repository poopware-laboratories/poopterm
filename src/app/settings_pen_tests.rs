//! Tests of the Settings window's widgets, read from the scene they record:
//! which buttons, where, lit or not, and the text inside them.
use super::{settings_pen::Pen, settings_view::ROW_HEIGHT};
use crate::{
	fonts::{BUNDLED, Font},
	render::{
		glyphs::FontSet,
		painter::ButtonStyle,
		scene::{Command, Rect, Scene},
	},
};

const STYLE: ButtonStyle = ButtonStyle {
	scale: 1,
	background: 0x101010,
	border: 0x808080,
	accent: 0x3060ff,
};

/// Draws with a pen at `scale` on a 900×600 logical window and returns the
/// recorded commands.
fn record(scale: usize, draw: impl FnOnce(&mut Pen, &mut Scene, ButtonStyle)) -> Vec<Command> {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pen = Pen {
		fonts: &mut fonts,
		px: 18.0 * scale as f32,
		ascent: 14 * scale as i32,
		color: 0xeeeeee,
		scale,
	};
	let mut scene = Scene::new(900 * scale, 600 * scale);
	draw(&mut pen, &mut scene, ButtonStyle { scale, ..STYLE });
	scene.commands
}

fn buttons(commands: &[Command]) -> Vec<(Rect, bool)> {
	let button = |command: &Command| match *command {
		Command::Button(rect, _, lit) => Some((rect, lit)),
		_ => None,
	};
	commands.iter().filter_map(button).collect()
}

fn texts(commands: &[Command]) -> Vec<(String, (usize, usize))> {
	let text = |command: &Command| match command {
		Command::Text { text, at, .. } => Some((text.clone(), *at)),
		_ => None,
	};
	commands.iter().filter_map(text).collect()
}

fn inside((x, y): (usize, usize), (rx, ry, rw, rh): Rect) -> bool {
	x >= rx && y >= ry && x < rx + rw && y < ry + rh
}

#[test]
fn pen_text_scales_its_row_and_size_but_not_its_column() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, _| pen.text(p, 30, 50, "Fonts", 0.5));
		let [Command::Text { at, px, ascent, .. }] = commands.as_slice() else {
			panic!("{commands:?}");
		};
		assert_eq!(
			(*at, *px, *ascent),
			((30, 50 * scale), 9.0 * scale as f32, 14 * scale as i32)
		);
	}
}

#[test]
fn a_toggle_lights_its_button_only_when_enabled_and_says_so() {
	for (enabled, word) in [(true, "Enabled"), (false, "Disabled")] {
		let commands = record(1, |pen, p, style| pen.toggle(p, style, 200, enabled));
		assert_eq!(buttons(&commands), [((660, 200, 216, ROW_HEIGHT), enabled)]);
		let texts = texts(&commands);
		assert_eq!(texts.len(), 1);
		assert_eq!(texts[0].0, word);
		assert!(inside(texts[0].1, (660, 200, 216, ROW_HEIGHT)), "{texts:?}");
	}
}

#[test]
fn a_stepper_puts_its_value_between_minus_and_plus() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, style| pen.stepper(p, style, 300, "12 px"));
		let s = scale;
		let row = (300 * s, 48 * s, ROW_HEIGHT * s);
		let minus = ((900 - 240) * s, row.0, row.1, row.2);
		let plus = ((900 - 72) * s, row.0, row.1, row.2);
		assert_eq!(buttons(&commands), [(minus, false), (plus, false)]);
		let texts = texts(&commands);
		let words: Vec<_> = texts.iter().map(|(text, _)| text.as_str()).collect();
		assert_eq!(words, ["−", "+", "12 px"]);
		assert!(
			inside(texts[0].1, minus) && inside(texts[1].1, plus),
			"x{s}: {texts:?}"
		);
		let value_x = texts[2].1.0;
		assert!(
			minus.0 + minus.2 <= value_x && value_x < plus.0,
			"x{s}: {texts:?}"
		);
	}
}

#[test]
fn a_wide_button_lights_when_active_or_hovered() {
	let rect = (660, 120, 216, ROW_HEIGHT);
	for (active, over, lit) in [
		(false, false, false),
		(true, false, true),
		(false, true, true),
	] {
		let hovered = |x, y, w, h| over && (x, y, w, h) == rect;
		let commands = record(1, |pen, p, style| {
			pen.wide_button(p, style, 120, "Ctrl+T", active, &hovered)
		});
		assert_eq!(
			buttons(&commands),
			[(rect, lit)],
			"active {active} hovered {over}"
		);
		let texts = texts(&commands);
		assert_eq!(texts[0].0, "Ctrl+T");
		assert!(inside(texts[0].1, rect), "{texts:?}");
	}
}

#[test]
fn the_back_button_sits_top_right_and_lights_under_the_pointer() {
	for scale in [1, 2] {
		let s = scale;
		let rect = ((900 - 164) * s, 20 * s, 140 * s, 36 * s);
		for over in [false, true] {
			let hovered = |x, y, w, h| over && (x, y, w, h) == rect;
			let commands = record(scale, |pen, p, style| pen.back_button(p, style, &hovered));
			assert_eq!(buttons(&commands), [(rect, over)]);
			let texts = texts(&commands);
			assert_eq!(texts[0].0, "Settings");
			assert!(inside(texts[0].1, rect), "x{s}: {texts:?}");
		}
	}
}

#[test]
fn the_footer_saves_at_the_bottom_right_and_shows_the_status_at_the_left() {
	for scale in [1, 2] {
		let s = scale;
		let save = ((900 - 140) * s, (600 - 52) * s, 120 * s, 36 * s);
		let commands = record(scale, |pen, p, style| {
			pen.footer(p, style, "Saved", &|_, _, _, _| false)
		});
		assert_eq!(buttons(&commands), [(save, false)]);
		let texts = texts(&commands);
		assert_eq!(texts[0].0, "Save");
		assert!(inside(texts[0].1, save), "x{s}: {texts:?}");
		assert_eq!(texts[1], ("Saved".to_owned(), (24 * s, (600 - 40) * s)));
	}
}

#[test]
fn centered_text_sits_inside_its_rectangle() {
	for label in ["A", "Themes", "Shortcuts"] {
		let rect = (100, 20, 120, 36);
		let commands = record(1, |pen, p, _| pen.centered(p, rect, label, 0.76));
		let texts = texts(&commands);
		let (x, y) = texts[0].1;
		let left = x - rect.0;
		// Centered on the base line, 1.2 times the base size, whatever the
		// label's size (TODO.md: "Center the chrome").
		let line = (18.0 * 1.2) as usize;
		let (above, below) = (y - rect.1, (rect.1 + rect.3) - (y + line));
		assert!(
			above.abs_diff(below) <= 4,
			"{label}: {above} above, {below} below"
		);
		assert!(inside(texts[0].1, rect), "{label}: {texts:?}");
		assert!(
			left > 0 && left < rect.2 / 2,
			"{label} starts in the left half: {texts:?}"
		);
	}
}
