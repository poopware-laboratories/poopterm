//! OSC 66, kitty's text sizing protocol: text drawn scaled over blocks of
//! cells.

use crate::term::{
	Term,
	multicell::{MAX_TEXT, Size},
};

impl Term {
	pub(super) fn osc_text_size(&mut self, params: &[&[u8]]) {
		let Some((&metadata, text)) = params.split_first() else {
			return;
		};
		let text = text.join(&b';');
		let Some(size) = Size::parse(metadata) else {
			return;
		};
		if text.len() > MAX_TEXT {
			return;
		}
		let text: String = String::from_utf8_lossy(&text)
			.chars()
			.filter(|c| !c.is_control())
			.collect();
		if size == Size::default() {
			self.print_text(text.as_str().into());
		} else if size.width > 0 {
			self.print_multicell(&text, size);
		} else {
			for (cluster, width) in clusters(&text) {
				let width = width as u8;
				self.print_multicell(cluster, Size { width, ..size });
			}
		}
	}
}

/// Splits text into characters with the marks after them, and sequences
/// joined by ZWJ, each with its width in cells.
fn clusters(text: &str) -> Vec<(&str, usize)> {
	let mut clusters: Vec<(usize, usize, usize)> = Vec::new();
	let mut joined = false;
	for (start, c) in text.char_indices() {
		let end = start + c.len_utf8();
		let width = crate::width::width(c);
		match clusters.last_mut() {
			Some(last) if width == 0 || joined => {
				last.1 = end;
				// Emoji presentation is two cells wide.
				if c == '\u{fe0f}' {
					last.2 = 2;
				}
			}
			// A mark with nothing before it has nothing to join.
			_ if width == 0 => {}
			_ => clusters.push((start, end, width)),
		}
		joined = c == '\u{200d}';
	}
	clusters
		.into_iter()
		.map(|(start, end, width)| (&text[start..end], width))
		.collect()
}
