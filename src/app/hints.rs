//! Quick select in the window: starting it over the focused pane, sending
//! it keys, and drawing its labels. Every decision is in `ui::hint_mode`.
use super::*;
use crate::{
	render::{pen::Pen, scene::paint},
	ui::{
		hint_mode::{HintMode, Outcome},
		hints::rows_of,
		links::LinkAction,
	},
};

impl App {
	/// Labels what is worth selecting in the focused pane; nothing happens
	/// when there is none.
	pub(super) fn start_hints(&mut self) {
		// pr-check: window glue: reads the live screen into the tested model
		let mut frame = Frame::default();
		self.active_term().snapshot(&mut frame);
		self.end_search();
		self.hint_mode = HintMode::new(&rows_of(&frame));
		self.request_redraw();
		// pr-check: end window glue
	}

	/// Sends `key` to quick select; false when it is not shown.
	pub(super) fn hint_key(&mut self, key: &Key) -> bool {
		// pr-check: window glue: hands a real key to the tested model
		let Some(mode) = &mut self.hint_mode else {
			return false;
		};
		let outcome = mode.press(key);
		if outcome != Outcome::Stay {
			self.hint_mode = None;
		}
		if let Outcome::Use(action) = outcome {
			self.use_hint(&action);
		}
		self.request_redraw();
		true
		// pr-check: end window glue
	}

	/// Opens or copies what quick select chose, as the model decided.
	fn use_hint(&self, action: &LinkAction) {
		// pr-check: window glue: hands the chosen text to the desktop
		let result = match action {
			LinkAction::OpenUrl(url) => open_external(url),
			LinkAction::OpenPath(path) => open_external(&path.to_string_lossy()),
			LinkAction::CopyPath(text) => clipboard_write(text),
		};
		if let Err(error) = result {
			crate::warnings::warning!("quick select could not use it: {error}");
		}
		// pr-check: end window glue
	}

	/// Each label over its target in the focused pane: the accent behind
	/// the letters left to type.
	pub(super) fn draw_hints(&mut self, p: &mut impl Pen) {
		// pr-check: window glue: draws the chips the tested model placed
		let Some(mode) = &self.hint_mode else { return };
		let crate::graphics::Rect { x, y, .. } = self.focused_rect();
		let chips = mode.chips(((x, y), (self.cw, self.ch)), self.active_term().cols);
		let (accent, text, px, ascent) = (self.theme.palette[3], self.theme.bg, self.px, self.ascent);
		paint(p, &mut self.fonts, |scene, _| {
			for ((cx, cy, w, h), letters) in chips {
				scene.rect(cx, cy, w, h, accent);
				scene.mono_text(cx, cy, letters, px, ascent, text);
			}
		});
		// pr-check: end window glue
	}
}
