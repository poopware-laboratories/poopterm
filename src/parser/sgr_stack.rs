//! XTPUSHSGR (`CSI Pm # {`) and XTPOPSGR (`CSI # }`): the rendition saved
//! on a stack of up to ten levels, each restoring all of it or the parts
//! its parameters chose.
use super::params::ParamList;
use crate::term::{STACK_DEPTH, Term, attr::*};

/// Parts kept outside `attrs`.
const BOLD: u8 = 1;
const REVERSE: u8 = 2;
const FOREGROUND: u8 = 4;
const BACKGROUND: u8 = 8;
const UNDERLINE_COLOR: u8 = 16;

/// The rendition one push saved, and the parts its pop restores.
#[derive(Clone, Debug)]
pub(crate) struct SavedSgr {
	parts: u8,
	attr_mask: u8,
	bold: bool,
	reverse: bool,
	attrs: u8,
	fg: (u32, u32),
	bg: u32,
	underline_color: Option<u32>,
}

impl Term {
	/// XTPUSHSGR: saves the parts `params` choose by their SGR numbers, or
	/// everything without any; pushes past ten levels are ignored.
	pub(super) fn push_sgr(&mut self, params: &(impl ParamList + ?Sized)) {
		if self.sgr_stack.len() >= STACK_DEPTH {
			return;
		}
		let (mut parts, mut attr_mask) = (0, 0);
		if params.is_default() {
			(parts, attr_mask) = (u8::MAX, u8::MAX);
		}
		for selector in params.iter().filter_map(|param| param.first()) {
			match selector {
				1 => parts |= BOLD,
				7 => parts |= REVERSE,
				30 => parts |= FOREGROUND,
				31 => parts |= BACKGROUND,
				2 => attr_mask |= DIM,
				3 => attr_mask |= ITALIC,
				// Single and double underline share their bits here.
				4 | 21 => attr_mask |= UNDERLINE | DOUBLE_UNDERLINE,
				5 => attr_mask |= BLINK,
				8 => attr_mask |= CONCEAL,
				9 => attr_mask |= STRIKE,
				_ => {}
			}
		}
		self.sgr_stack.push(SavedSgr {
			parts,
			attr_mask,
			bold: self.bold,
			reverse: self.reverse,
			attrs: self.attrs,
			fg: (self.fg, self.fg_id),
			bg: self.bg,
			underline_color: self.underline_color,
		});
	}

	/// XTPOPSGR: restores what the last push chose, if anything was pushed.
	pub(super) fn pop_sgr(&mut self) {
		let Some(saved) = self.sgr_stack.pop() else {
			return;
		};
		let chosen = |part: u8| saved.parts & part != 0;
		if chosen(BOLD) {
			self.bold = saved.bold;
		}
		if chosen(REVERSE) {
			self.reverse = saved.reverse;
		}
		if chosen(FOREGROUND) {
			(self.fg, self.fg_id) = saved.fg;
		}
		if chosen(BACKGROUND) {
			self.bg = saved.bg;
		}
		if chosen(UNDERLINE_COLOR) {
			self.set_underline_color(saved.underline_color);
		}
		self.attrs = self.attrs & !saved.attr_mask | saved.attrs & saved.attr_mask;
	}
}
