//! Text and widgets shared by the Settings window's pages.
use super::settings_view::ROW_HEIGHT;
use crate::render::{painter::ButtonStyle, scene::Scene};

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

	/// Draws `text` centered in the physical rectangle `rect`.
	pub(super) fn centered(
		&mut self,
		p: &mut Scene,
		rect: (usize, usize, usize, usize),
		text: &str,
		size: f32,
	) {
		let advance = self
			.fonts
			.primary()
			.metrics('M', self.px * size)
			.advance_width;
		let text_w = (advance * text.chars().count() as f32) as usize;
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
		let s = self.scale;
		let back_x = p.width.saturating_sub(164 * s);
		p.button(
			(back_x, 20 * s, 140 * s, 36 * s),
			style,
			hovered(back_x, 20 * s, 140 * s, 36 * s),
		);
		self.text(p, back_x + 22 * s, 27, "Settings", 0.8);
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

	/// The Save button and the status line at the bottom of every page.
	pub(super) fn footer(
		&mut self,
		p: &mut Scene,
		style: ButtonStyle,
		status: &str,
		hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
	) {
		let s = self.scale;
		let (save_x, save_y) = (
			p.width.saturating_sub(140 * s),
			p.height.saturating_sub(52 * s),
		);
		let (save_w, save_h) = (120 * s, 36 * s);
		p.button(
			(save_x, save_y, save_w, save_h),
			style,
			hovered(save_x, save_y, save_w, save_h),
		);
		let cell_width = self
			.fonts
			.primary()
			.metrics('M', self.px)
			.advance_width
			.ceil() as usize;
		let save_text_x = save_x + save_w.saturating_sub(4 * cell_width) / 2;
		p.text(
			self.fonts,
			save_text_x,
			save_y + 6 * s,
			"Save",
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
