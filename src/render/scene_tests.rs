//! Tests of scenes: a recorded scene keeps every operation in order, and
//! replaying it paints exactly what drawing directly paints.
use super::{
	chrome,
	glyphs::FontSet,
	painter::{ButtonStyle, Painter},
	pen::Pen,
	scene::{Command, Scene},
};
use crate::fonts::{BUNDLED, Font};

const BACKGROUND: u32 = 0x123456;

/// Chrome painted directly, with a [`Scene`]'s methods, for tests to compare
/// with a replayed scene.
impl Painter<'_> {
	pub(crate) fn button(
		&mut self,
		rect: (usize, usize, usize, usize),
		style: ButtonStyle,
		hovered: bool,
	) {
		chrome::button(self, rect, style, hovered);
	}

	/// A line of text whose top edge is `y`; `ascent` is the baseline offset.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text(
		&mut self,
		fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
	) {
		let (width, height) = self.size();
		self.text_clipped(fonts, x, y, text, px, ascent, color, (0, 0, width, height));
	}

	/// A line of text clipped to `clip`.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text_clipped(
		&mut self,
		fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
		clip: (usize, usize, usize, usize),
	) {
		chrome::text_clipped(self, fonts, (x, y), text, px, ascent, color, clip);
	}
}

fn style(scale: usize) -> ButtonStyle {
	ButtonStyle {
		scale,
		background: 0x101010,
		border: 0x808080,
		accent: 0x3060ff,
	}
}

fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new())
}

/// Runs `$body` with `$p` as a [`Painter`] on a `$width`×`$height` buffer,
/// then again recording into a [`Scene`] that is replayed on a second
/// buffer, and checks both give the same pixels; evaluates to the scene.
macro_rules! assert_replays {
	(($width:expr, $height:expr), |$p:ident, $fonts:ident| $body:expr) => {{
		let (width, height): (usize, usize) = ($width, $height);
		let mut direct = vec![BACKGROUND; width * height];
		{
			let $fonts = &mut fonts();
			let $p = &mut Painter::new(&mut direct, width, height);
			$body;
		}
		let mut scene = Scene::new(width, height);
		{
			let $fonts = &mut fonts();
			let $p = &mut scene;
			$body;
		}
		let mut replayed = vec![BACKGROUND; width * height];
		scene.replay(
			&mut Painter::new(&mut replayed, width, height),
			&mut fonts(),
		);
		let differ = (0..direct.len())
			.filter(|&i| direct[i] != replayed[i])
			.count();
		assert_eq!(differ, 0, "{differ} pixels differ");
		assert!(
			direct.iter().any(|&pixel| pixel != BACKGROUND),
			"nothing drawn"
		);
		scene
	}};
}

#[test]
fn a_new_scene_is_empty_and_keeps_its_size() {
	let scene = Scene::new(40, 30);
	assert_eq!((scene.width, scene.height), (40, 30));
	assert!(scene.commands.is_empty());
}

#[test]
fn a_scene_records_each_operation_in_order() {
	let mut scene = Scene::new(40, 30);
	let fonts = &mut fonts();
	scene.fill(1);
	scene.rect(1, 2, 3, 4, 5);
	scene.round_rect(6, 7, 8, 9, 2, 10);
	scene.button((2, 3, 30, 12), style(2), true);
	scene.text_clipped(fonts, 3, 4, "hi", 12.0, 9, 11, (1, 1, 20, 20));
	scene.mono_text(5, 6, "log", 10.0, 8, 12);
	assert_eq!(
		scene.commands,
		[
			Command::Fill(1),
			Command::Rect((1, 2, 3, 4), 5),
			Command::RoundRect((6, 7, 8, 9), 2, 10),
			Command::Button((2, 3, 30, 12), style(2), true),
			Command::Text {
				at: (3, 4),
				text: "hi".to_owned(),
				px: 12.0,
				ascent: 9,
				color: 11,
				clip: (1, 1, 20, 20),
				mono: false,
			},
			Command::Text {
				at: (5, 6),
				text: "log".to_owned(),
				px: 10.0,
				ascent: 9 - 1,
				color: 12,
				clip: (0, 0, 40, 30),
				mono: true,
			},
		]
	);
}

#[test]
fn unclipped_text_is_recorded_clipped_to_the_whole_target() {
	let mut scene = Scene::new(64, 20);
	scene.text(&mut fonts(), 2, 3, "poop", 12.0, 9, 0xffffff);
	let Command::Text { clip, text, .. } = &scene.commands[0] else {
		panic!("{:?}", scene.commands);
	};
	assert_eq!((*clip, text.as_str()), ((0, 0, 64, 20), "poop"));
}

#[test]
fn recorded_text_owns_a_verbatim_copy_of_multibyte_empty_and_spaced_strings() {
	let mut scene = Scene::new(64, 20);
	let fonts = &mut fonts();
	let mut title = String::from("héllo 🐱 世界");
	scene.text(fonts, 0, 0, &title, 12.0, 9, 1);
	scene.text(fonts, 0, 0, "", 12.0, 9, 1);
	scene.text(fonts, 0, 0, "  spaced out ", 12.0, 9, 1);
	title.clear();
	let texts: Vec<_> = scene
		.commands
		.iter()
		.map(|command| match command {
			Command::Text { text, .. } => text.as_str(),
			other => panic!("{other:?}"),
		})
		.collect();
	assert_eq!(texts, ["héllo 🐱 世界", "", "  spaced out "]);
}

#[test]
fn replayed_primitives_paint_what_direct_drawing_paints() {
	assert_replays!((120, 50), |p, fonts| {
		p.fill(0x0a0a0a);
		p.button((75, 4, 40, 22), style(1), true);
		p.button((75, 26, 40, 22), style(1), false);
		p.rect(5, 5, 30, 10, 0xff0000);
		p.round_rect(40, 4, 30, 20, 6, 0x00ff00);
		p.text(fonts, 4, 22, "Hxg poop", 13.0, 10, 0xffffff);
		p.text_clipped(
			fonts,
			4,
			34,
			"clipped away",
			13.0,
			10,
			0xffff00,
			(4, 34, 30, 12),
		);
	});
}

#[test]
fn replayed_buttons_match_at_every_scale_hovered_or_not_and_too_small() {
	for scale in 1..=3 {
		for hovered in [false, true] {
			assert_replays!((60 * scale, 40 * scale), |p, _fonts| {
				p.button((2, 2, 50 * scale, 24 * scale), style(scale), hovered);
				p.button((2, 30 * scale, 3, 1), style(scale), hovered);
			});
		}
	}
}

#[test]
fn a_fill_replays_over_everything_before_it_and_under_everything_after() {
	let mut buffer = vec![BACKGROUND; 8 * 4];
	let mut scene = Scene::new(8, 4);
	scene.rect(0, 0, 8, 4, 0xff0000);
	scene.fill(0x00ff00);
	scene.rect(0, 0, 2, 2, 0x0000ff);
	scene.replay(&mut Painter::new(&mut buffer, 8, 4), &mut fonts());
	assert_eq!(
		[buffer[0], buffer[2], buffer[31]],
		[0x0000ff, 0x00ff00, 0x00ff00]
	);
}

#[test]
fn later_operations_replay_over_earlier_ones() {
	let mut buffer = vec![BACKGROUND; 20 * 10];
	let mut scene = Scene::new(20, 10);
	scene.rect(0, 0, 10, 10, 0xff0000);
	scene.rect(5, 0, 10, 10, 0x00ff00);
	scene.replay(&mut Painter::new(&mut buffer, 20, 10), &mut fonts());
	assert_eq!(
		[buffer[2], buffer[7], buffer[12], buffer[17]],
		[0xff0000, 0x00ff00, 0x00ff00, BACKGROUND]
	);
}

#[test]
fn shapes_running_off_the_target_replay_clipped_the_same() {
	assert_replays!((30, 20), |p, fonts| {
		p.rect(20, 10, 50, 50, 0xff0000);
		p.round_rect(25, 15, 40, 40, 8, 0x00ff00);
		p.text(fonts, 22, 2, "WWWWWWWW", 13.0, 10, 0xffffff);
	});
}

#[test]
fn text_clipped_to_a_box_replays_the_same_at_every_edge() {
	let clips = [
		(0, 0, 30, 20),
		(6, 3, 10, 8),
		(0, 0, 1, 1),
		(29, 19, 5, 5),
		(40, 40, 10, 10),
	];
	for clip in clips {
		let mut scene = Scene::new(30, 20);
		let mut direct = vec![BACKGROUND; 30 * 20];
		let fonts = &mut fonts();
		Painter::new(&mut direct, 30, 20).text_clipped(fonts, 2, 2, "Wg", 14.0, 11, 0xffffff, clip);
		scene.text_clipped(fonts, 2, 2, "Wg", 14.0, 11, 0xffffff, clip);
		let mut replayed = vec![BACKGROUND; 30 * 20];
		scene.replay(&mut Painter::new(&mut replayed, 30, 20), fonts);
		assert_eq!(direct, replayed, "clip {clip:?}");
	}
}

#[test]
fn an_empty_scene_replays_nothing() {
	let mut buffer = vec![BACKGROUND; 8 * 8];
	Scene::new(8, 8).replay(&mut Painter::new(&mut buffer, 8, 8), &mut fonts());
	assert!(buffer.iter().all(|&pixel| pixel == BACKGROUND));
}

/// A tiny deterministic generator, so a failing sequence can be replayed.
fn next(state: &mut u64) -> usize {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	(*state >> 33) as usize
}

#[test]
fn random_overlapping_operations_replay_like_direct_drawing() {
	let mut state = 0x9e37_79b9_7f4a_7c15;
	for _ in 0..40 {
		let ops: Vec<[usize; 6]> = (0..12)
			.map(|_| std::array::from_fn(|_| next(&mut state)))
			.collect();
		assert_replays!((48, 32), |p, fonts| {
			for &[kind, x, y, w, h, color] in &ops {
				let (x, y, w, h, color) = (x % 56, y % 40, w % 40, h % 30, color as u32 & 0xffffff);
				match kind % 5 {
					0 => p.rect(x, y, w, h, color),
					1 => p.round_rect(x, y, w, h, color as usize % 20, color),
					2 => p.button((x, y, w, h), style(1 + kind % 3), kind % 2 == 0),
					3 => p.text(fonts, x, y, "Ag 🐱", 9.0 + (w % 12) as f32, 8, color),
					_ => p.text_clipped(fonts, x, y, "poop", 12.0, 9, color, (w, h, 20, 12)),
				}
			}
		});
	}
}

/// `text` replayed as mono text at `(x, y)` on a `width`×`height` buffer
/// with `fonts`.
fn mono_pixels(
	fonts: &mut FontSet,
	(width, height): (usize, usize),
	(x, y): (usize, usize),
	text: &str,
) -> Vec<u32> {
	let mut scene = Scene::new(width, height);
	scene.mono_text(x, y, text, 14.0, 11, 0xffffff);
	let mut pixels = vec![BACKGROUND; width * height];
	scene.replay(&mut Painter::new(&mut pixels, width, height), fonts);
	pixels
}

#[test]
fn mono_text_replays_as_drawing_it_directly_does_even_off_the_edge() {
	for x in [2, 20, 28] {
		let fonts = &mut fonts().with_mono(Font::from_bytes(BUNDLED, 0).unwrap());
		let mut direct = vec![BACKGROUND; 30 * 20];
		let whole = (0, 0, 30, 20);
		let painter = &mut Painter::new(&mut direct, 30, 20);
		chrome::mono_clipped(painter, fonts, (x, 2), "Wg", 14.0, 11, 0xffffff, whole);
		assert_eq!(direct, mono_pixels(fonts, (30, 20), (x, 2), "Wg"), "x {x}");
		assert!(
			direct.iter().any(|&pixel| pixel != BACKGROUND),
			"x {x}: nothing drawn"
		);
	}
}

#[test]
fn without_a_mono_face_mono_text_looks_like_text() {
	let fonts = &mut fonts();
	let mut text = vec![BACKGROUND; 40 * 20];
	let mut scene = Scene::new(40, 20);
	scene.text(fonts, 2, 2, "Wig", 14.0, 11, 0xffffff);
	scene.replay(&mut Painter::new(&mut text, 40, 20), fonts);
	assert_eq!(text, mono_pixels(fonts, (40, 20), (2, 2), "Wig"));
}

#[test]
fn mono_text_the_mono_face_lacks_comes_from_the_usual_fonts() {
	// The bundled font has no '中': both draw the primary font's notdef.
	let fonts = &mut fonts().with_mono(Font::from_bytes(BUNDLED, 0).unwrap());
	let mut text = vec![BACKGROUND; 40 * 20];
	let mut scene = Scene::new(40, 20);
	scene.text(fonts, 2, 2, "中", 14.0, 11, 0xffffff);
	scene.replay(&mut Painter::new(&mut text, 40, 20), fonts);
	assert_eq!(text, mono_pixels(fonts, (40, 20), (2, 2), "中"));
	assert_eq!(fonts.mono_cached(), 0);
}

#[test]
fn mono_text_spans_the_whole_scene_after_it_is_recorded() {
	let mut scene = Scene::new(40, 30);
	scene.mono_text(0, 0, "x", 10.0, 8, 1);
	scene.width = 80;
	scene.mono_text(0, 0, "y", 10.0, 8, 1);
	let clips: Vec<_> = scene
		.commands
		.iter()
		.map(|command| match command {
			Command::Text {
				clip, mono: true, ..
			} => *clip,
			other => panic!("{other:?}"),
		})
		.collect();
	assert_eq!(
		clips,
		[(0, 0, 40, 30), (0, 0, 80, 30)],
		"the size when recorded"
	);
}
