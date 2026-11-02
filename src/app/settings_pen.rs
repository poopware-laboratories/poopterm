//! Text and widgets shared by the Settings window's pages.
use super::settings_view::ROW_HEIGHT;
use crate::render::{
	painter::ButtonStyle,
	scene::{Rect, Scene},
};

/// The "Settings" back button of a window `width` physical pixels wide.
pub(super) fn back_rect(width: usize, s: usize) -> Rect {
	(width.saturating_sub(164 * s), 20 * s, 140 * s, 36 * s)
}

/// The footer's button, `button_w` logical pixels wide, in a window
/// `(width, height)` physical pixels large: 20 from the right edge.
pub(super) fn footer_rect((width, height): (usize, usize), button_w: usize, s: usize) -> Rect {
	let w = button_w * s;
	(
		width.saturating_sub(w + 20 * s),
		height.saturating_sub(52 * s),
		w,
		36 * s,
	)
}

/// Text drawing for one Settings frame; every label shares the base ascent.
pub(super) struct Pen<'a> {
	pub(super) fonts: &'a mut crate::render::glyphs::FontSet,
	pub(super) px: f32,
	pub(super) ascent: i32,
	pub(super) color: u32,
	pub(super) scale: usize,
}

impl Pen<'_> {
	/// Draws `text` at logical `(x, y)` scaled by `size` relative to the base px.
	pub(super) fn text(&mut self, p: &mut Scene, x: usize, y: usize, text: &str, size: f32) {
		p.text(
			self.fonts,
			x,
			y * self.scale,
			text,
			self.px * size,
			self.ascent,
			self.color,
		);
	}

	/// How wide `text` is at `size` times the base size, in physical
	/// pixels: the sum of the primary font's advances.
	pub(super) fn width(&mut self, text: &str, size: f32) -> usize {
		let (font, px) = (self.fonts.primary(), self.px * size);
		let advances = text.chars().map(|c| font.metrics(c, px).advance_width);
		advances.sum::<f32>().ceil() as usize
	}

	/// Draws `text` centered in the physical rectangle `rect`.
	pub(super) fn centered(
		&mut self,
		p: &mut Scene,
		rect: (usize, usize, usize, usize),
		text: &str,
		size: f32,
	) {
		let text_w = self.width(text, size);
		let y = (rect.1 + rect.3.saturating_sub((self.px * 1.2) as usize) / 2) / self.scale;
		self.text(p, rect.0 + rect.2.saturating_sub(text_w) / 2, y, text, size);
	}

	/// The "Settings" button that leads back to the main page.
	pub(super) fn back_button(
		&mut self,
		p: &mut Scene,
		style: ButtonStyle,
		hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
	) {
		let (s, back) = (self.scale, back_rect(p.width, self.scale));
		p.button(back, style, hovered(back.0, back.1, back.2, back.3));
		self.text(p, back.0 + 22 * s, 27, "Settings", 0.8);
	}

	/// A wide button at the right of `row` labelled `label`.
	pub(super) fn wide_button(
		&mut self,
		p: &mut Scene,
		style: ButtonStyle,
		row: usize,
		label: &str,
		active: bool,
		hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
	) {
		let s = self.scale;
		let x = p.width.saturating_sub(240 * s);
		let (y, w, h) = (row * s, 216 * s, ROW_HEIGHT * s);
		p.button((x, y, w, h), style, active || hovered(x, y, w, h));
		self.text(p, x + 12 * s, row + 10, label, 0.76);
	}

	/// `−` and `+` buttons at the right of `row` with `value` between them.
	pub(super) fn stepper(&mut self, p: &mut Scene, style: ButtonStyle, row: usize, value: &str) {
		let s = self.scale;
		for (offset, sign) in [(240, "−"), (72, "+")] {
			let x = p.width.saturating_sub(offset * s);
			p.button((x, row * s, 48 * s, ROW_HEIGHT * s), style, false);
			self.text(p, x + 16 * s, row + 7, sign, 1.0);
		}
		self.text(p, p.width.saturating_sub(180 * s), row + 8, value, 0.82);
	}

	/// An Enabled/Disabled toggle at the right of `row`.
	pub(super) fn toggle(&mut self, p: &mut Scene, style: ButtonStyle, row: usize, enabled: bool) {
		let s = self.scale;
		let x = p.width.saturating_sub(240 * s);
		p.button((x, row * s, 216 * s, ROW_HEIGHT * s), style, enabled);
		let state = if enabled { "Enabled" } else { "Disabled" };
		self.text(p, x + 16 * s, row + 8, state, 0.82);
	}

	/// How many characters of text `size` times the base size fit in
	/// `width` physical pixels, counting each as wide as `M`.
	pub(super) fn columns(&mut self, width: usize, size: f32) -> usize {
		let advance = self
			.fonts
			.primary()
			.metrics('M', self.px * size)
			.advance_width;
		(width as f32 / advance.max(1.0)) as usize
	}

	/// The footer at the bottom of every page: its `button`, labelled and
	/// as wide as given, at the right, and the status line at the left.
	pub(super) fn footer(
		&mut self,
		p: &mut Scene,
		style: ButtonStyle,
		(label, width): (&str, usize),
		status: &str,
		hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
	) {
		let s = self.scale;
		let (save_x, save_y, save_w, save_h) = footer_rect((p.width, p.height), width, s);
		p.button(
			(save_x, save_y, save_w, save_h),
			style,
			hovered(save_x, save_y, save_w, save_h),
		);
		let label_w = self.width(label, 1.0);
		let save_text_x = save_x + save_w.saturating_sub(label_w) / 2;
		p.text(
			self.fonts,
			save_text_x,
			save_y + 6 * s,
			label,
			self.px,
			self.ascent,
			self.color,
		);
		let status_y = p.height.saturating_sub(40 * s);
		p.text(
			self.fonts,
			24 * s,
			status_y,
			status,
			self.px * 0.68,
			self.ascent,
			self.color,
		);
	}
}
