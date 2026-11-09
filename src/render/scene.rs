//! Drawing recorded as data: window chrome records a scene, which the
//! software renderer replays into pixels and a GPU renderer can draw.
#[cfg(doc)]
use super::painter::Painter;
use super::{
	chrome,
	glyphs::FontSet,
	painter::ButtonStyle,
	pen::{Ink, Pen},
	window_icons::WindowIcon,
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
	/// A rectangle the keyboard can focus, which paints nothing.
	Focusable(Rect),
	/// A line of text whose top edge is `at.1` and baseline `ascent` below
	/// it, clipped to `clip`.
	Text {
		at: (usize, usize),
		text: String,
		px: f32,
		ascent: i32,
		color: u32,
		clip: Rect,
		/// Drawn in the fonts' monospace face.
		mono: bool,
	},
	/// A window button's icon, its top-left corner at `at`.
	Icon {
		at: (usize, usize),
		icon: WindowIcon,
		scale: usize,
		color: u32,
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

/// Records what `draw` draws into a scene the size of `p`, then replays it
/// into `p`.
pub(crate) fn paint(
	p: &mut impl Pen,
	fonts: &mut FontSet,
	draw: impl FnOnce(&mut Scene, &mut FontSet),
) {
	let (width, height) = p.size();
	let mut scene = Scene::new(width, height);
	draw(&mut scene, fonts);
	scene.replay(p, fonts);
}

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

	/// Marks `rect` as something the keyboard can focus. It paints nothing.
	pub(crate) fn focusable(&mut self, rect: Rect) {
		self.commands.push(Command::Focusable(rect));
	}

	/// The rectangles the keyboard can focus, in drawing order: every button
	/// and every rectangle marked with [`Self::focusable`].
	pub(crate) fn focus_rects(&self) -> Vec<Rect> {
		let rect = |command: &Command| match *command {
			Command::Button(rect, ..) | Command::Focusable(rect) => Some(rect),
			_ => None,
		};
		self.commands.iter().filter_map(rect).collect()
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
			mono: false,
		});
	}

	/// A line of text in the fonts' monospace face, as [`Self::text`] draws
	/// proportional text.
	pub(crate) fn mono_text(
		&mut self,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
	) {
		self.commands.push(Command::Text {
			at: (x, y),
			text: text.to_owned(),
			px,
			ascent,
			color,
			clip: (0, 0, self.width, self.height),
			mono: true,
		});
	}

	/// A window button's icon, `12 * scale` pixels square, at `at`.
	pub(crate) fn icon(&mut self, at: (usize, usize), icon: WindowIcon, scale: usize, color: u32) {
		(self.commands).push(Command::Icon {
			at,
			icon,
			scale,
			color,
		});
	}

	/// This scene moved `dx` pixels right, on a target `dx` wider, to paste
	/// its operations into a wider scene. A fill becomes a rectangle over
	/// the area it covered.
	pub(crate) fn shifted(self, dx: usize) -> Scene {
		let (width, height) = (self.width, self.height);
		let right = |(x, y, w, h): Rect| (x + dx, y, w, h);
		let commands = self.commands.into_iter().map(|command| match command {
			Command::Fill(color) => Command::Rect((dx, 0, width, height), color),
			Command::Rect(rect, color) => Command::Rect(right(rect), color),
			Command::RoundRect(rect, radius, color) => Command::RoundRect(right(rect), radius, color),
			Command::Button(rect, style, lit) => Command::Button(right(rect), style, lit),
			Command::Focusable(rect) => Command::Focusable(right(rect)),
			Command::Text {
				at,
				text,
				px,
				ascent,
				color,
				clip,
				mono,
			} => Command::Text {
				at: (at.0 + dx, at.1),
				text,
				px,
				ascent,
				color,
				clip: right(clip),
				mono,
			},
			Command::Icon {
				at,
				icon,
				scale,
				color,
			} => Command::Icon {
				at: (at.0 + dx, at.1),
				icon,
				scale,
				color,
			},
		});
		Scene {
			width: width + dx,
			height,
			commands: commands.collect(),
		}
	}

	/// Draws the scene with `p`, pixel for pixel as drawing each operation
	/// with a [`Painter`] directly would.
	pub(crate) fn replay(&self, p: &mut impl Pen, fonts: &mut FontSet) {
		for command in &self.commands {
			match *command {
				Command::Fill(color) => {
					let (width, height) = p.size();
					p.rect(0, 0, width, height, color);
				}
				Command::Button(rect, style, hovered) => chrome::button(p, rect, style, hovered),
				Command::Focusable(_) => {}
				Command::Rect((x, y, w, h), color) => p.rect(x, y, w, h, color),
				Command::RoundRect(rect, r, color) => p.round_rect(rect, r, color),
				Command::Text {
					at,
					ref text,
					px,
					ascent,
					color,
					clip,
					mono: false,
				} => chrome::text_clipped(p, fonts, at, text, px, ascent, color, clip),
				Command::Text {
					at,
					ref text,
					px,
					ascent,
					color,
					clip,
					mono: true,
				} => chrome::mono_clipped(p, fonts, at, text, px, ascent, color, clip),
				Command::Icon {
					at: (x, y),
					icon,
					scale,
					color,
				} => {
					let glyph = fonts.window_icon(icon, scale);
					let (x, y) = (x as i32, y as i32);
					let (w, h) = (glyph.metrics.width as i32, glyph.metrics.height as i32);
					let clip = (x, y, x + w, y + h);
					p.glyph(glyph, Ink::Icon(icon, scale), (x, y), clip, false, color);
				}
			}
		}
	}
}
