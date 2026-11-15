//! Operating System Commands: titles, hyperlinks and cwd; colors, the
//! clipboard and notifications have their own modules.
use crate::term::Term;

impl Term {
	pub(super) fn apply_osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		let Some(command) = params.first().copied() else {
			return;
		};
		if self.apply_osc_color(command, &params[1..], bell) {
			return;
		}
		match command {
			b"52" if params.len() >= 3 => self.osc_selection(params[1], params[2]),
			b"5522" if params.len() >= 2 => {
				self.osc_kitty_clipboard(params[1], params.get(2).copied().unwrap_or_default());
			}
			b"9" => self.osc_notify(&params[1..]),
			b"777" => self.osc_notify_777(&params[1..]),
			b"22" => self.osc_pointer(params.get(1).copied().unwrap_or_default(), bell),
			b"7501" => self.osc_program_status(&params[1..], bell),
			b"99" if params.len() >= 3 => self.osc_kitty_notify(params[1], &params[2..], bell),
			b"8" if params.len() >= 3 => {
				// An empty, rejected or unstorable target ends the open link.
				let uri = std::str::from_utf8(params[2])
					.ok()
					.filter(|uri| !uri.is_empty() && uri.len() <= 2048 && !uri.chars().any(char::is_control));
				self.active_hyperlink = uri.and_then(|uri| self.hyperlink_id(uri)).unwrap_or(0);
				self.update_active_extra();
			}
			b"133" => {
				let letter = params.get(1).copied().unwrap_or_default();
				if matches!(letter, b"L" | b"A" | b"N") {
					self.fresh_line();
				}
				let secondary = params[2.min(params.len())..]
					.iter()
					.any(|option| option.starts_with(b"k=") && *option != b"k=i");
				if matches!(letter, b"A" | b"N") && !secondary {
					self.status.prompt();
				}
				self.shell.record(
					params,
					self.history_base + self.scrollback.len() + self.cy,
					self.cx,
					std::time::Instant::now(),
				);
			}
			b"0" | b"1" | b"2" if params.len() >= 2 => {
				// The last arm: nothing after the match is skipped.
				let Some(title) = self.decode_title(params[1]) else {
					return;
				};
				if params[0] != b"2" {
					self.icon_name.clone_from(&title);
				}
				if params[0] != b"1" {
					self.set_title(title);
				}
			}
			b"7" if params.len() >= 2 => self.osc_cwd_uri(params[1]),
			b"633" => self.osc_vscode(&params[1..]),
			b"3008" => self.contexts.apply(&params[1..]),
			b"1337" if params.len() >= 2 => self.osc_iterm(&params[1..], bell),
			b"21337" => self.osc_session_status(&params[1..]),
			b"66" => self.osc_text_size(&params[1..]),
			_ => {}
		}
	}
}

impl Term {
	/// The slot of hyperlink target `uri`, added unless the table is full.
	fn hyperlink_id(&mut self, uri: &str) -> Option<u16> {
		if let Some(&id) = self.hyperlink_ids.get(uri) {
			return Some(id);
		}
		if self.hyperlinks.len() >= 4096 {
			return None;
		}
		let id = self.hyperlinks.len() as u16;
		self.hyperlinks.push(uri.to_owned());
		self.hyperlink_ids.insert(uri.to_owned(), id);
		Some(id)
	}

	/// Sets the window title, redrawing the tab if it changed.
	pub(super) fn set_title(&mut self, title: String) {
		if self.title != title {
			self.title = title;
			self.title_generation = self.title_generation.wrapping_add(1);
		}
	}
}

/// A title or icon name without control characters, at most 64 characters.
pub(super) fn clean_title(text: &[u8]) -> String {
	// Most titles are short printable ASCII, copied as they are.
	if text.len() <= 64 && text.iter().all(|&byte| matches!(byte, b' '..=b'~')) {
		return String::from_utf8_lossy(text).into_owned();
	}
	String::from_utf8_lossy(text)
		.chars()
		.filter(|c| !c.is_control())
		.take(64)
		.collect()
}

#[cfg(test)]
mod tests {
	use super::clean_title;

	#[test]
	fn titles_lose_controls_and_stop_at_64_characters() {
		assert_eq!(clean_title(b"vim main.rs"), "vim main.rs");
		assert_eq!(clean_title(b"a\x7fb\tc"), "abc");
		assert_eq!(clean_title("é\u{85}ü".as_bytes()), "éü");
		assert_eq!(clean_title(b"bad \xff byte"), "bad \u{fffd} byte");
		let long = "x".repeat(70);
		assert_eq!(clean_title(long.as_bytes()), "x".repeat(64));
		let wide = "é".repeat(70);
		assert_eq!(clean_title(wide.as_bytes()), "é".repeat(64));
	}
}
