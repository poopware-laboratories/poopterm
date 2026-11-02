//! Select Graphic Rendition (`CSI … m`): colors and text attributes.
use std::sync::Arc;

use super::params::ParamList;
use crate::{
	term::{NO_EXTRA, Term, attr::*},
	theme::xterm_color,
};

impl Term {
	pub(crate) fn sgr(&mut self, params: &(impl ParamList + ?Sized)) {
		if !self.special_colors_active() {
			return self.apply_sgr(params);
		}
		// Special colors replace the foreground SGR chose; start from it.
		self.fg = self.sgr_fg;
		self.apply_sgr(params);
		self.sgr_fg = self.fg;
		self.apply_special_colors();
	}

	#[inline(always)]
	fn apply_sgr(&mut self, params: &(impl ParamList + ?Sized)) {
		let group_count = params.group_count();
		if group_count == 0 {
			self.reset_style();
			return;
		}
		let mut i = 0;
		while i < group_count {
			let group = params.get(i).unwrap_or_default();
			let Some(&code) = group.first() else {
				self.reset_style();
				i += 1;
				continue;
			};
			// 58 (underline color) takes the same arguments.
			if matches!(code, 38 | 48 | 58) {
				let (mode, color, consumed) = if group.len() > 1 {
					match group.get(1).copied() {
						Some(5) if group.len() >= 3 => (5, Some(u32::from(group[2].min(255))), 0),
						Some(2) if group.len() >= 5 => {
							let start = if group.len() >= 6 && group[2] == 0 {
								3
							} else {
								2
							};
							let color = (group.len() >= start + 3)
								.then(|| rgb_color(group[start], group[start + 1], group[start + 2]));
							(2, color, 0)
						}
						_ => (0, None, 0),
					}
				} else if let Some(mode_group) = params.get(i + 1) {
					let mode = mode_group.first().copied().unwrap_or(0);
					match mode {
						5 => (
							5,
							params
								.get(i + 2)
								.and_then(|g| g.first())
								.map(|v| u32::from((*v).min(255))),
							2,
						),
						2 => {
							let color = params
								.get(i + 2)
								.and_then(|r| r.first())
								.zip(params.get(i + 3).and_then(|g| g.first()))
								.zip(params.get(i + 4).and_then(|b| b.first()))
								.map(|((r, g), b)| rgb_color(*r, *g, *b));
							(2, color, 4)
						}
						_ => (0, None, 0),
					}
				} else {
					(0, None, 0)
				};
				// `color` is an RGB value, or a palette index for mode 5.
				let resolved = color.map(|color| match mode {
					5 => self.indexed_color(color as u8),
					_ => color,
				});
				match (code, resolved) {
					(38, Some(resolved)) => (self.fg, self.fg_id) = (resolved, color.unwrap_or(0)),
					(48, Some(resolved)) => self.bg = resolved,
					(58, Some(resolved)) => self.set_underline_color(Some(resolved)),
					_ => {}
				}
				if mode != 0 {
					i += consumed;
				}
				i += 1;
				continue;
			}
			if code == 4 && group.len() > 1 {
				match group[1] {
					0 => self.attrs &= !(UNDERLINE | DOUBLE_UNDERLINE),
					2 => self.attrs |= UNDERLINE | DOUBLE_UNDERLINE,
					_ => self.attrs |= UNDERLINE,
				}
				i += 1;
				continue;
			}
			match code {
				0 => self.reset_style(),
				1 => self.bold = true,
				2 => self.attrs |= DIM,
				3 => self.attrs |= ITALIC,
				4 => self.attrs |= UNDERLINE,
				21 => self.attrs |= UNDERLINE | DOUBLE_UNDERLINE,
				5 | 6 => self.attrs |= BLINK,
				7 => self.reverse = true,
				8 => self.attrs |= CONCEAL,
				9 => self.attrs |= STRIKE,
				22 => {
					self.bold = false;
					self.attrs &= !DIM;
				}
				23 => self.attrs &= !ITALIC,
				24 => self.attrs &= !(UNDERLINE | DOUBLE_UNDERLINE),
				25 => self.attrs &= !BLINK,
				27 => self.reverse = false,
				28 => self.attrs &= !CONCEAL,
				29 => self.attrs &= !STRIKE,
				59 => self.set_underline_color(None),
				53 => self.attrs |= OVERLINE,
				55 => self.attrs &= !OVERLINE,
				30..=37 => {
					(self.fg, self.fg_id) = (self.indexed_color((code - 30) as u8), code as u32 - 30)
				}
				39 => (self.fg, self.fg_id) = (self.default_fg, 0),
				40..=47 => self.bg = self.indexed_color((code - 40) as u8),
				49 => self.bg = self.default_bg,
				90..=97 => {
					(self.fg, self.fg_id) = (self.indexed_color((code - 90 + 8) as u8), code as u32 - 82)
				}
				100..=107 => self.bg = self.indexed_color((code - 100 + 8) as u8),
				_ => {}
			}
			i += 1;
		}
	}
}

impl Term {
	pub(super) fn reset_style(&mut self) {
		self.fg = self.default_fg;
		self.fg_id = 0;
		self.bg = self.default_bg;
		self.attrs = 0;
		self.bold = false;
		self.reverse = false;
		self.set_underline_color(None);
	}

	pub(crate) fn set_underline_color(&mut self, color: Option<u32>) {
		if self.underline_color != color {
			self.underline_color = color;
			self.update_active_extra();
		}
	}

	/// Points [`Term::active_extra`] at the active hyperlink and underline
	/// color; with the table full, printed cells go without both.
	pub(super) fn update_active_extra(&mut self) {
		if self.active_hyperlink == 0 && self.underline_color.is_none() {
			self.active_extra = NO_EXTRA;
			return;
		}
		if self.underline_color.is_none()
			&& let Some(id) = self.extras.remembered_link(self.active_hyperlink)
		{
			self.active_extra = id;
			return;
		}
		let extras = Arc::make_mut(&mut self.extras);
		self.active_extra = extras
			.with_hyperlink(NO_EXTRA, self.active_hyperlink)
			.and_then(|id| extras.with_underline_color(id, self.underline_color))
			.unwrap_or_default();
	}

	pub(crate) fn indexed_color(&self, index: u8) -> u32 {
		self.color_overrides[index as usize]
			.unwrap_or_else(|| xterm_color(index, self.palette, self.bright))
	}
}

fn rgb_color(r: u16, g: u16, b: u16) -> u32 {
	((r.min(255) as u32) << 16) | ((g.min(255) as u32) << 8) | b.min(255) as u32
}
