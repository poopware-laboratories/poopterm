//! Tests of the Settings window's widgets, read from the scene they record:
//! which buttons, where, lit or not, and the text inside them.
use super::{
	settings_pen::{Pen, segment_at, segment_rect, segmented_rect, switch_hit, switch_rect},
	settings_view::ROW_HEIGHT,
};
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
pub(super) fn record(
	scale: usize,
	draw: impl FnOnce(&mut Pen, &mut Scene, ButtonStyle),
) -> Vec<Command> {
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

pub(super) fn buttons(commands: &[Command]) -> Vec<(Rect, bool)> {
	let button = |command: &Command| match *command {
		Command::Button(rect, _, lit) => Some((rect, lit)),
		_ => None,
	};
	commands.iter().filter_map(button).collect()
}

pub(super) fn texts(commands: &[Command]) -> Vec<(String, (usize, usize))> {
	let text = |command: &Command| match command {
		Command::Text { text, at, .. } => Some((text.clone(), *at)),
		_ => None,
	};
	commands.iter().filter_map(text).collect()
}

pub(super) fn inside((x, y): (usize, usize), (rx, ry, rw, rh): Rect) -> bool {
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

/// The round rectangles a switch records, as `(rect, radius, color)`.
fn pills(commands: &[Command]) -> Vec<(Rect, usize, u32)> {
	commands
		.iter()
		.filter_map(|command| match *command {
			Command::RoundRect(rect, radius, color) => Some((rect, radius, color)),
			_ => None,
		})
		.collect()
}

#[test]
fn a_switch_is_a_pill_track_with_a_round_knob_at_the_row_right() {
	for scale in [1, 2] {
		let s = scale;
		let commands = record(scale, |pen, p, style| pen.switch(p, style, 200, false));
		let drawn = pills(&commands);
		assert_eq!(drawn.len(), 2, "x{s}");
		let track = ((900 - 58) * s, 210 * s, 34 * s, 20 * s);
		assert_eq!((drawn[0].0, drawn[0].1), (track, 10 * s), "x{s} track");
		let knob = (track.0 + 2 * s, track.1 + 2 * s, 16 * s, 16 * s);
		assert_eq!((drawn[1].0, drawn[1].1), (knob, 8 * s), "x{s} knob");
	}
}

#[test]
fn the_knob_slides_to_the_right_end_when_on() {
	let at = |on| {
		let commands = record(1, |pen, p, style| pen.switch(p, style, 200, on));
		pills(&commands)[1].0
	};
	assert_eq!(at(true).0 - at(false).0, 14);
	assert_eq!((at(true).1, at(true).2), (at(false).1, at(false).2));
	let track = pills(&record(1, |pen, p, style| pen.switch(p, style, 200, true)))[0].0;
	assert_eq!(
		at(true).0 + at(true).2 + 2,
		track.0 + track.2,
		"two pixels from the track's end"
	);
}

#[test]
fn the_track_takes_the_accent_when_on_and_a_quiet_shade_when_off() {
	let color = |on| {
		let commands = record(1, |pen, p, style| pen.switch(p, style, 200, on));
		pills(&commands)[0].2
	};
	assert_eq!(color(true), STYLE.accent);
	assert_ne!(color(false), STYLE.accent);
	assert_ne!(color(false), STYLE.background);
	let knobs: Vec<_> = [true, false]
		.into_iter()
		.map(|on| pills(&record(1, |pen, p, style| pen.switch(p, style, 200, on)))[1].2)
		.collect();
	assert_eq!(knobs, [0xffffff, 0xffffff]);
}

#[test]
fn a_switch_says_nothing_and_draws_no_button() {
	let commands = record(1, |pen, p, style| pen.switch(p, style, 200, true));
	assert!(buttons(&commands).is_empty());
	assert!(texts(&commands).is_empty());
}

#[test]
fn the_switch_is_centered_on_its_row() {
	for row in [100, 250, 468] {
		let commands = record(1, |pen, p, style| pen.switch(p, style, row, true));
		let (_, y, _, h) = pills(&commands)[0].0;
		assert_eq!(y - row, ROW_HEIGHT - (y - row) - h, "row {row}");
	}
}

#[test]
fn the_click_area_is_the_switch_with_eight_pixels_either_side_and_the_whole_row() {
	for scale in [1, 2] {
		let s = scale;
		let (track_x, _, track_w, _) = switch_rect(900 * s, 200, s);
		let hit = switch_hit(900 * s, 200, s);
		assert_eq!(
			hit,
			(track_x - 8 * s, 200 * s, track_w + 16 * s, ROW_HEIGHT * s)
		);
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
fn the_footer_saves_at_the_bottom_right_and_shows_the_status_at_the_left() {
	for scale in [1, 2] {
		let s = scale;
		let save = ((900 - 140) * s, (600 - 52) * s, 120 * s, 36 * s);
		let commands = record(scale, |pen, p, style| {
			pen.footer(p, style, ("Save", 120), "Saved", &|_, _, _, _| false)
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

/// What one character of the bundled monospaced font costs when fitting
/// at `size`: its advance, rounded up.
fn cell(pen: &mut Pen, size: f32) -> usize {
	let px = pen.px * size;
	pen.fonts.primary().metrics('M', px).advance_width.ceil() as usize
}

#[test]
fn fitting_keeps_text_that_fits_and_cuts_the_rest_within_the_width() {
	let text = "renderer = \"software\" in config.toml or POOPTERM_RENDERER";
	let length = text.chars().count();
	for (scale, size) in [(1, 0.72), (2, 0.68), (1, 1.3)] {
		record(scale, |pen, _, _| {
			let c = cell(pen, size);
			assert_eq!(pen.fit(text, length * c, size), text);
			for room in [length * c - 1, length * c / 2, length * c / 5, c] {
				let fitted = pen.fit(text, room, size);
				let kept = fitted
					.strip_suffix('…')
					.expect("cut text ends in an ellipsis");
				assert!(text.starts_with(kept), "{fitted:?}");
				assert!(pen.width(&fitted, size) <= room, "{fitted:?} in {room}");
				// As many characters as fit beside the ellipsis, less spaces
				// cut from the end.
				let most = room / c - 1;
				assert!(kept.chars().count() <= most, "{fitted:?} in {room}");
				assert_eq!(kept, text.chars().take(most).collect::<String>().trim_end());
			}
			assert_eq!(pen.fit(text, c - 1, size), "", "not even the ellipsis fits");
			assert_eq!(pen.fit("", 0, size), "");
		});
	}
}

#[test]
fn fitting_measures_each_character_of_the_text() {
	record(1, |pen, _, _| {
		let c = cell(pen, 0.72);
		assert_eq!(pen.fit("abcdefghij", 10 * c, 0.72), "abcdefghij");
		assert_eq!(pen.fit("abcdefghijk", 10 * c, 0.72), "abcdefghi…");
		assert_eq!(pen.fit("abcdefghijk", 11 * c, 0.72), "abcdefghijk");
	});
}

#[test]
fn a_footer_button_of_any_width_keeps_the_right_margin_and_centers_its_label() {
	for scale in [1, 2] {
		let s = scale;
		let commands = record(scale, |pen, p, style| {
			pen.footer(p, style, ("Copy report", 140), "", &|_, _, _, _| false)
		});
		let copy = ((900 - 160) * s, (600 - 52) * s, 140 * s, 36 * s);
		assert_eq!(buttons(&commands), [(copy, false)]);
		let (label, (x, _)) = texts(&commands).swap_remove(0);
		assert_eq!(label, "Copy report");
		let (left, right) = (x - copy.0, (copy.0 + copy.2) - x);
		assert!(
			left > 0 && left < right,
			"x{s}: starts in the left half at {x}"
		);
	}
}

#[test]
fn a_hovered_footer_button_lights() {
	let commands = record(1, |pen, p, style| {
		pen.footer(p, style, ("Save", 120), "", &|x, _, _, _| x == 760)
	});
	assert_eq!(buttons(&commands), [((760, 548, 120, 36), true)]);
}

#[test]
fn width_sums_the_advances_of_every_character() {
	for (scale, size) in [(1, 0.76), (2, 0.76), (1, 1.0)] {
		record(scale, |pen, _, _| {
			let font = Font::from_bytes(BUNDLED, 0).unwrap();
			let px = pen.px * size;
			let advance = |c| font.metrics(c, px).advance_width;
			for text in ["", "a", "Diagnostics", "Copy report", "é€"] {
				let sum: f32 = text.chars().map(advance).sum();
				assert_eq!(pen.width(text, size), sum.ceil() as usize, "{text:?}");
			}
		});
	}
}

#[test]
fn centered_labels_leave_equal_room_on_both_sides() {
	for scale in [1, 2] {
		for label in ["Log", "Graphics", "Diagnostics"] {
			let rect = (24 * scale, 76 * scale, 96 * scale, 32 * scale);
			let commands = record(scale, |pen, p, _| {
				pen.centered(p, rect, label, 0.76);
				let width = pen.width(label, 0.76);
				let (x, _) = texts(&p.commands)[0].1;
				let (left, right) = (x - rect.0, rect.0 + rect.2 - (x + width));
				assert!(
					left.abs_diff(right) <= 1,
					"x{scale} {label}: {left} | {right}"
				);
			});
			assert_eq!(texts(&commands).len(), 1);
		}
	}
}

/// The bundled font's advance at `px`, rounded up as text drawing moves.
fn bundled_advance(px: f32) -> usize {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	font.metrics('M', px).advance_width.ceil() as usize
}

/// Each recorded line's place, color, size and face.
fn lines(commands: &[Command]) -> Vec<((usize, usize), u32, f32, bool)> {
	let line = |command: &Command| match *command {
		Command::Text {
			at,
			color,
			px,
			mono,
			..
		} => (at, color, px, mono),
		ref other => panic!("{other:?}"),
	};
	commands.iter().map(line).collect()
}

#[test]
fn tinted_text_is_drawn_in_its_color_and_the_pen_keeps_its_own() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, _| {
			pen.tinted(p, 30, 50, "Muted", 0.5, 0x123456);
			pen.text(p, 30, 80, "Plain", 0.5);
		});
		let px = 9.0 * scale as f32;
		assert_eq!(
			lines(&commands),
			[
				((30, 50 * scale), 0x123456, px, false),
				((30, 80 * scale), 0xeeeeee, px, false)
			]
		);
	}
}

#[test]
fn mono_text_is_recorded_in_the_mono_face_at_the_pens_scale() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, _| {
			pen.mono(p, 37, 165, "10:00:00 UTC", 12.0 / 18.0, 0x654321);
		});
		let [
			Command::Text {
				at,
				text,
				px,
				ascent,
				color,
				clip,
				mono,
			},
		] = commands.as_slice()
		else {
			panic!("{commands:?}");
		};
		assert_eq!((*at, text.as_str()), ((37, 165 * scale), "10:00:00 UTC"));
		assert_eq!((*px, *ascent), (12.0 * scale as f32, 14 * scale as i32));
		assert_eq!((*color, *mono), (0x654321, true));
		assert_eq!(*clip, (0, 0, 900 * scale, 600 * scale), "the whole window");
	}
}

#[test]
fn mono_text_leaves_the_pens_color_alone() {
	let commands = record(1, |pen, p, _| {
		pen.mono(p, 0, 0, "a", 1.0, 0x111111);
		pen.text(p, 0, 20, "b", 1.0);
	});
	assert_eq!(lines(&commands)[1].1, 0xeeeeee);
}

#[test]
fn mono_width_sums_each_characters_advance_rounded_up() {
	for scale in [1, 2] {
		let advance = bundled_advance(12.0 * scale as f32);
		let mut widths = (1, 0, 0);
		record(scale, |pen, _, _| {
			let size = 12.0 / 18.0;
			widths = (
				pen.mono_width("", size),
				pen.mono_width("ab", size),
				pen.mono_width("10:00:00 UTC", size),
			);
		});
		assert_eq!(widths, (0, 2 * advance, 12 * advance), "x{scale}");
	}
}

#[test]
fn mono_fit_keeps_what_fits_and_cuts_the_rest_with_an_ellipsis() {
	let size = 12.0 / 18.0;
	let advance = bundled_advance(12.0);
	record(1, |pen, _, _| {
		assert_eq!(pen.mono_fit("abcdef", 6 * advance, size), "abcdef");
		let cut = pen.mono_fit("abcdefgh", 6 * advance, size);
		assert_eq!(cut, "abcde…", "five characters beside the ellipsis");
		assert!(pen.mono_width(&cut, size) <= 6 * advance);
		assert_eq!(
			pen.mono_fit("ab   cdefgh", 6 * advance, size),
			"ab…",
			"no trailing spaces"
		);
		assert_eq!(
			pen.mono_fit("abc", advance - 1, size),
			"",
			"not even the ellipsis fits"
		);
		assert_eq!(pen.mono_fit("", 0, size), "");
	});
}

#[test]
fn mono_fit_measures_at_the_pens_scale() {
	let size = 12.0 / 18.0;
	let advance = bundled_advance(24.0);
	record(2, |pen, _, _| {
		assert_eq!(pen.mono_fit("abcd", 4 * advance, size), "abcd");
		assert_eq!(
			pen.mono_fit("abcd", 4 * advance - 1, size),
			"ab…",
			"the ellipsis is a full advance"
		);
	});
}

/// The one text command `centered_row` records for `text` in a window
/// `width` logical pixels wide at `scale`.
fn centered(scale: usize, width: usize, text: &str, size: f32) -> (usize, usize, u32, usize) {
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pen = Pen {
		fonts: &mut fonts,
		px: 18.0 * scale as f32,
		ascent: 14 * scale as i32,
		color: 0xeeeeee,
		scale,
	};
	let mut scene = Scene::new(width * scale, 600 * scale);
	pen.centered_row(&mut scene, (200, size, 0x123456), text);
	let drawn = pen.width(text, size);
	match scene.commands.as_slice() {
		[Command::Text { at, color, .. }] => (at.0, at.1, *color, drawn),
		other => panic!("{other:?}"),
	}
}

#[test]
fn a_centered_row_leaves_equal_margins_either_side() {
	for scale in [1, 2] {
		for text in [
			"poopterm",
			"A small, native terminal emulator written in Rust.",
		] {
			let (x, _, _, width) = centered(scale, 760, text, 0.82);
			let (left, right) = (x, 760 * scale - (x + width));
			assert!(
				left.abs_diff(right) <= 1,
				"{text:?} at {scale}x: {left} against {right}"
			);
		}
	}
}

#[test]
fn a_centered_row_takes_its_row_and_color_as_given() {
	let (_, y, color, _) = centered(2, 760, "x", 1.0);
	assert_eq!((y, color), (400, 0x123456));
}

#[test]
fn a_wider_line_starts_further_left() {
	let (short, ..) = centered(1, 760, "About", 0.82);
	let (long, ..) = centered(1, 760, "About poopterm and its authors", 0.82);
	assert!(long < short);
}

#[test]
fn a_line_wider_than_the_window_starts_at_the_left_edge() {
	let (x, ..) = centered(1, 40, "A line far too long for a window this narrow", 0.82);
	assert_eq!(x, 0);
}

#[test]
fn a_larger_size_is_wider_and_so_starts_further_left() {
	let (small, ..) = centered(1, 760, "poopterm", 0.8);
	let (large, ..) = centered(1, 760, "poopterm", 1.5);
	assert!(large < small);
}

const LABELS: [&str; 2] = ["Top", "Side"];

#[test]
fn the_control_is_a_156_by_28_track_24_from_the_right_edge_centered_on_the_row() {
	for scale in [1, 2] {
		let s = scale;
		assert_eq!(
			segmented_rect(900 * s, 200, s),
			((900 - 24 - 156) * s, 206 * s, 156 * s, 28 * s),
			"x{s}"
		);
	}
}

#[test]
fn segments_split_the_track_evenly_inside_a_two_pixel_inset() {
	let (x, y, w, h) = segmented_rect(900, 200, 1);
	assert_eq!(segment_rect(900, 200, 1, 0, 2), (x + 2, y + 2, 76, h - 4));
	assert_eq!(
		segment_rect(900, 200, 1, 1, 2),
		(x + 2 + 76, y + 2, 76, h - 4)
	);
	let (sx, _, sw, _) = segment_rect(900, 200, 1, 1, 2);
	assert_eq!(
		sx + sw,
		x + w - 2,
		"the pair ends at the track's inset edge"
	);
}

#[test]
fn the_segments_of_any_count_never_overlap_or_leave_the_track() {
	for count in 2..=4 {
		for scale in [1, 2] {
			let (tx, _, tw, _) = segmented_rect(900 * scale, 200, scale);
			let mut edge = tx;
			for index in 0..count {
				let (x, _, w, _) = segment_rect(900 * scale, 200, scale, index, count);
				assert!(x >= edge, "{count} segments, #{index} at {scale}x");
				assert!(x + w <= tx + tw, "{count} segments, #{index} at {scale}x");
				edge = x + w;
			}
		}
	}
}

#[test]
fn a_point_names_the_segment_it_is_on_and_nothing_outside_the_track() {
	let (x, y, w, h) = segmented_rect(900, 200, 1);
	let at = |px, py| segment_at(900, 200, 1, (px, py), 2);
	assert_eq!(at(x + 4, y + 4), Some(0));
	assert_eq!(at(x + w / 2 - 1, y + h / 2), Some(0));
	assert_eq!(at(x + w / 2, y + h / 2), Some(1));
	assert_eq!(at(x + w - 1, y + h - 1), Some(1));
	assert_eq!(at(x - 1, y + 4), None);
	assert_eq!(at(x + w, y + 4), None);
	assert_eq!(at(x + 4, y - 1), None);
	assert_eq!(at(x + 4, y + h), None);
}

#[test]
fn a_two_segment_control_has_a_second_half_that_names_the_second_segment_at_twice_the_scale() {
	let (x, y, w, h) = segmented_rect(1800, 200, 2);
	assert_eq!(segment_at(1800, 200, 2, (x + w - 3, y + h / 2), 2), Some(1));
	assert_eq!(segment_at(1800, 200, 2, (x + 3, y + h / 2), 2), Some(0));
}

#[test]
fn the_track_and_the_selected_segment_are_the_only_fills_and_the_selected_one_moves() {
	for selected in [0, 1] {
		let commands = record(1, |pen, p, style| {
			pen.segmented(p, style, 200, &LABELS, selected)
		});
		let drawn = pills(&commands);
		assert_eq!(drawn.len(), 2, "selected {selected}");
		assert_eq!(drawn[0].0, segmented_rect(900, 200, 1));
		assert_eq!(
			drawn[1].0,
			segment_rect(900, 200, 1, selected, 2),
			"selected {selected}"
		);
	}
}

#[test]
fn the_selected_segment_is_lighter_than_its_track_and_both_follow_the_style() {
	let commands = record(1, |pen, p, style| pen.segmented(p, style, 200, &LABELS, 0));
	let drawn = pills(&commands);
	assert_ne!(drawn[0].2, drawn[1].2);
	assert_ne!(
		drawn[0].2, STYLE.background,
		"the track stands apart from the page"
	);
	let lighter = |color: u32| (color >> 16 & 0xff) + (color >> 8 & 0xff) + (color & 0xff);
	assert!(lighter(drawn[1].2) > lighter(drawn[0].2));
}

#[test]
fn each_label_is_centered_in_its_segment_and_only_the_selected_is_full_strength() {
	for selected in [0, 1] {
		let commands = record(1, |pen, p, style| {
			pen.segmented(p, style, 200, &LABELS, selected)
		});
		let labels = texts(&commands);
		let words: Vec<_> = labels.iter().map(|(text, _)| text.as_str()).collect();
		assert_eq!(words, LABELS);
		for (index, (_, at)) in labels.iter().enumerate() {
			assert!(
				inside(*at, segment_rect(900, 200, 1, index, 2)),
				"{index}: {at:?}"
			);
		}
		let colors: Vec<u32> = commands
			.iter()
			.filter_map(|command| match command {
				Command::Text { color, .. } => Some(*color),
				_ => None,
			})
			.collect();
		assert_ne!(colors[0], colors[1]);
		assert_eq!(colors[selected], 0xeeeeee, "the pen's own color");
	}
}

#[test]
fn a_segmented_control_draws_no_buttons() {
	let commands = record(1, |pen, p, style| pen.segmented(p, style, 200, &LABELS, 1));
	assert!(buttons(&commands).is_empty());
}

#[test]
fn every_pixel_of_the_control_maps_to_the_segment_drawn_under_it() {
	for scale in [1, 2] {
		let (x, y, w, h) = segmented_rect(900 * scale, 200, scale);
		for py in y..y + h {
			for px in x..x + w {
				let named = segment_at(900 * scale, 200, scale, (px, py), 2).expect("on the track");
				let rect = segment_rect(900 * scale, 200, scale, named, 2);
				let inset =
					(rect.0..rect.0 + rect.2).contains(&px) && (rect.1..rect.1 + rect.3).contains(&py);
				// The two pixel border belongs to the nearest segment.
				let border = px < rect.0 + rect.2 + 2 * scale && px + 2 * scale >= rect.0;
				assert!(inset || border, "({px}, {py}) at {scale}x named {named}");
			}
		}
	}
}

#[test]
fn a_selection_past_the_labels_fills_no_segment() {
	let commands = record(1, |pen, p, style| pen.segmented(p, style, 200, &LABELS, 5));
	assert_eq!(pills(&commands).len(), 1, "only the track");
	assert_eq!(texts(&commands).len(), 2);
}

#[test]
fn a_control_with_no_labels_is_only_its_track() {
	let commands = record(1, |pen, p, style| pen.segmented(p, style, 200, &[], 0));
	assert_eq!(pills(&commands).len(), 1);
	assert!(texts(&commands).is_empty());
}

#[test]
fn a_control_of_three_labels_splits_into_thirds_and_names_each() {
	let labels = ["One", "Two", "Three"];
	let commands = record(1, |pen, p, style| pen.segmented(p, style, 200, &labels, 2));
	let drawn = pills(&commands);
	assert_eq!(drawn[1].0, segment_rect(900, 200, 1, 2, 3));
	for (index, (_, at)) in texts(&commands).iter().enumerate() {
		assert!(inside(*at, segment_rect(900, 200, 1, index, 3)), "{index}");
	}
	let (x, y, ..) = segmented_rect(900, 200, 1);
	for (point, want) in [(x + 10, 0), (x + 60, 1), (x + 120, 2)] {
		assert_eq!(
			segment_at(900, 200, 1, (point, y + 10), 3),
			Some(want),
			"x {point}"
		);
	}
}

#[test]
fn a_single_segment_takes_the_whole_track() {
	let (x, y, ..) = segmented_rect(900, 200, 1);
	assert_eq!(segment_at(900, 200, 1, (x + 100, y + 5), 1), Some(0));
	assert_eq!(segment_rect(900, 200, 1, 0, 1).2, 152);
}

#[test]
fn a_zero_count_never_divides_by_zero() {
	let (x, y, ..) = segmented_rect(900, 200, 1);
	assert_eq!(segment_at(900, 200, 1, (x + 5, y + 5), 0), Some(0));
	assert_eq!(segment_rect(900, 200, 1, 0, 0).2, 152);
}

#[test]
fn a_narrow_window_keeps_the_control_inside_it() {
	for width in [180, 200, 400] {
		let (x, _, w, _) = segmented_rect(width, 200, 1);
		assert!(x + w <= width, "{width}");
	}
	assert_eq!(
		segmented_rect(100, 200, 1).0,
		0,
		"saturates at the left edge"
	);
}

#[test]
fn labels_read_on_the_track_and_the_filled_segment_in_every_built_in_theme() {
	use crate::{
		render::blend::blend,
		theme::{Theme, ThemeKind},
		ui::contrast::{TEXT, ratio},
	};
	let mut kind = ThemeKind::MacOsDark;
	loop {
		let theme = Theme::from_kind(kind);
		let track = blend(theme.bg, theme.border, 70);
		let lit = blend(track, theme.fg, 40);
		let muted = blend(theme.fg, theme.bg, 70);
		assert!(
			ratio(theme.fg, lit) >= TEXT,
			"{kind:?} selected: {:.2}",
			ratio(theme.fg, lit)
		);
		assert!(
			ratio(muted, track) >= 3.0,
			"{kind:?} muted: {:.2}",
			ratio(muted, track)
		);
		assert!(
			ratio(track, theme.bg) > 1.02,
			"{kind:?} track against the page"
		);
		kind = kind.next();
		if kind == ThemeKind::MacOsDark {
			break;
		}
	}
}

fn focusables(commands: &[Command]) -> Vec<Rect> {
	commands
		.iter()
		.filter_map(|command| match *command {
			Command::Focusable(rect) => Some(rect),
			_ => None,
		})
		.collect()
}

#[test]
fn a_switch_can_be_focused_over_the_area_a_click_flips_it_in() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, style| pen.switch(p, style, 200, true));
		assert_eq!(
			focusables(&commands),
			[switch_hit(900 * scale, 200, scale)],
			"x{scale}"
		);
	}
}

#[test]
fn each_segment_can_be_focused_in_order() {
	for scale in [1, 2] {
		let commands = record(scale, |pen, p, style| {
			pen.segmented(p, style, 200, &LABELS, 0)
		});
		let wanted: Vec<_> = (0..2)
			.map(|i| segment_rect(900 * scale, 200, scale, i, 2))
			.collect();
		assert_eq!(focusables(&commands), wanted, "x{scale}");
	}
}

#[test]
fn a_switch_and_a_segmented_control_focus_where_a_click_on_them_lands() {
	use super::settings_focus::center;
	let on_switch = center(switch_hit(900, 200, 1));
	let (sx, sy, sw, sh) = switch_hit(900, 200, 1);
	assert!((sx..sx + sw).contains(&on_switch.0) && (sy..sy + sh).contains(&on_switch.1));
	for index in 0..2 {
		let middle = center(segment_rect(900, 200, 1, index, 2));
		assert_eq!(segment_at(900, 200, 1, middle, 2), Some(index));
	}
}

#[test]
fn a_stepper_and_a_footer_button_are_focusable_as_the_buttons_they_are() {
	let stepper = record(1, |pen, p, style| pen.stepper(p, style, 300, "12 px"));
	let rects: Vec<_> = buttons(&stepper)
		.into_iter()
		.map(|(rect, _)| rect)
		.collect();
	assert_eq!(rects.len(), 2);
	let footer = record(1, |pen, p, style| {
		pen.footer(p, style, ("Save", 120), "", &|_, _, _, _| false)
	});
	assert_eq!(buttons(&footer).len(), 1);
}
