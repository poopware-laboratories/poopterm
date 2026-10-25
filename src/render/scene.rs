//! Drawing recorded as data: window chrome records a scene, which the
//! software renderer replays into pixels and a GPU renderer can draw.
use super::{
	glyphs::FontSet,
	painter::{ButtonStyle, Painter},
};

/// A rectangle in physical pixels: x, y, width, height.
pub(crate) type Rect = (usize, usize, usize, usize);

/// One recorded drawing operation, in physical pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
	Fill(u32),
	Rect(Rect, u32),
	RoundRect(Rect, usize, u32),
	Button(Rect, ButtonStyle, bool),
	/// A line of text whose top edge is `at.1` and baseline `ascent` below
	/// it, clipped to `clip`.
	Text {
		at: (usize, usize),
		text: String,
		px: f32,
		ascent: i32,
		color: u32,
		clip: Rect,
	},
}

/// Operations recorded in order with [`Painter`]'s methods and arguments,
/// on a target `width` by `height` pixels.
#[derive(Debug, Default)]
pub(crate) struct Scene {
	pub width: usize,
	pub height: usize,
	pub commands: Vec<Command>,
}

#[cfg_attr(
	not(test),
	expect(dead_code, reason = "window chrome records scenes next (#30)")
)]
impl Scene {
	pub(crate) fn new(width: usize, height: usize) -> Self {
		Self {
			width,
			height,
			commands: Vec::new(),
		}
	}

	pub(crate) fn fill(&mut self, color: u32) {
		self.commands.push(Command::Fill(color));
	}

	pub(crate) fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		self.commands.push(Command::Rect((x, y, w, h), color));
	}

	pub(crate) fn round_rect(
		&mut self,
		x: usize,
		y: usize,
		w: usize,
		h: usize,
		r: usize,
		color: u32,
	) {
		self
			.commands
			.push(Command::RoundRect((x, y, w, h), r, color));
	}

	pub(crate) fn button(&mut self, rect: Rect, style: ButtonStyle, hovered: bool) {
		self.commands.push(Command::Button(rect, style, hovered));
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
		let all = (0, 0, self.width, self.height);
		self.text_clipped(fonts, x, y, text, px, ascent, color, all);
	}

	/// A line of text clipped to `clip`. Glyphs are found when the scene is
	/// drawn, so `fonts` is unused here; it keeps [`Painter`]'s arguments.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text_clipped(
		&mut self,
		_fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
		clip: Rect,
	) {
		self.commands.push(Command::Text {
			at: (x, y),
			text: text.to_owned(),
			px,
			ascent,
			color,
			clip,
		});
	}

	/// Paints the scene into `p`, pixel for pixel as drawing each operation
	/// there directly would.
	pub(crate) fn replay(&self, p: &mut Painter, fonts: &mut FontSet) {
		for command in &self.commands {
			match *command {
				Command::Fill(color) => p.fill(color),
				Command::Rect((x, y, w, h), color) => p.rect(x, y, w, h, color),
				Command::RoundRect((x, y, w, h), r, color) => p.round_rect(x, y, w, h, r, color),
				Command::Button(rect, style, hovered) => p.button(rect, style, hovered),
				Command::Text {
					at,
					ref text,
					px,
					ascent,
					color,
					clip,
				} => {
					p.text_clipped(fonts, at.0, at.1, text, px, ascent, color, clip);
				}
			}
		}
	}
}
