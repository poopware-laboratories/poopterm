//! Tests of the close prompt's model: when a close asks first, what it
//! says, which keys answer it, and the popover it draws.
use super::{
	close_prompt::{
		Answer, CloseRequest, CloseTarget, NOTE, answer, close_prompt, close_request, ends,
	},
	overlays::{Place, command_palette},
};
use crate::theme::{Theme, ThemeKind};
use winit::keyboard::{Key, NamedKey};

fn running(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| name.to_string()).collect()
}

fn message(names: &[&str]) -> Option<String> {
	close_request(CloseTarget::Pane(1), &running(names)).map(|request| request.message)
}

#[test]
fn nothing_running_closes_at_once() {
	assert_eq!(close_request(CloseTarget::Pane(3), &[]), None);
	assert_eq!(close_request(CloseTarget::Tab(3), &[]), None);
}

#[test]
fn one_program_is_named_and_the_target_kept() {
	for target in [CloseTarget::Pane(7), CloseTarget::Tab(9)] {
		let request = close_request(target, &running(&["vim"]));
		let expected = CloseRequest {
			target,
			message: "vim is still running".to_owned(),
		};
		assert_eq!(request, Some(expected));
	}
}

#[test]
fn several_programs_name_the_first_and_count_the_rest() {
	assert_eq!(
		message(&["htop", "cargo"]).as_deref(),
		Some("htop and 1 more are still running")
	);
	let three = message(&["vim", "less", "top"]);
	assert_eq!(three.as_deref(), Some("vim and 2 more are still running"));
}

#[test]
fn names_lose_control_characters_and_long_ones_are_cut() {
	// Names come from the system, which takes what a program calls itself.
	assert_eq!(
		message(&["vi\x1b[31mm\n"]).as_deref(),
		Some("vi[31mm is still running")
	);
	let long = "x".repeat(41);
	let cut = format!("{}… is still running", "x".repeat(40));
	assert_eq!(message(&[&long]), Some(cut));
	let exact = "y".repeat(40);
	assert_eq!(
		message(&[&exact]),
		Some(format!("{exact} is still running")),
		"no ellipsis"
	);
	// Cut by characters, so a multibyte name is never split inside one.
	let wide = "é".repeat(45);
	let cut = format!("{}… is still running", "é".repeat(40));
	assert_eq!(message(&[&wide]), Some(cut));
}

#[test]
fn enter_closes_escape_keeps_and_other_keys_wait() {
	let named = |key| Key::Named(key);
	assert_eq!(answer(&named(NamedKey::Enter), false), Answer::Close);
	assert_eq!(answer(&named(NamedKey::Escape), false), Answer::Keep);
	for key in [
		Key::Character("y".into()),
		named(NamedKey::Space),
		named(NamedKey::Tab),
	] {
		assert_eq!(answer(&key, false), Answer::Wait, "{key:?}");
	}
}

#[test]
fn the_close_shortcut_pressed_again_closes() {
	assert_eq!(answer(&Key::Character("w".into()), true), Answer::Close);
	// Even a close shortcut bound to Escape means close.
	assert_eq!(answer(&Key::Named(NamedKey::Escape), true), Answer::Close);
}

fn place(scale: usize) -> Place {
	Place {
		window: (1000 * scale, 600 * scale),
		top: 44 * scale,
		scale,
	}
}

#[test]
fn the_prompt_floats_where_the_palette_does_with_the_message_and_a_note() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	for scale in [1, 2] {
		let px = 14.0 * scale as f32;
		let prompt = close_prompt(place(scale), "vim is still running", px, &theme);
		let palette = command_palette(place(scale), ("", true), px, &theme);
		assert_eq!(prompt.rect, palette.rect);
		assert_eq!(prompt.field.rect, palette.field.rect);
		assert_eq!(prompt.field.text, "vim is still running");
		assert_eq!(prompt.field.placeholder, "");
		assert!(prompt.rows.is_empty() && prompt.buttons.is_empty());
		let (note_rect, note) = prompt.note.unwrap();
		assert_eq!((note_rect, note), (palette.note.unwrap().0, NOTE));
	}
}

/// Sessions 1 and 2 share tab 0, 3 is alone in tab 1, and 9 is in none.
fn tab_of(id: u64) -> Option<usize> {
	match id {
		1 | 2 => Some(0),
		3 => Some(1),
		_ => None,
	}
}

#[test]
fn closing_a_pane_ends_only_its_own_session() {
	let ended: Vec<u64> = (0..10)
		.filter(|&id| ends(CloseTarget::Pane(2), id, tab_of))
		.collect();
	assert_eq!(ended, [2]);
	// Even a session in no tab, as one whose shell already exited.
	assert!(ends(CloseTarget::Pane(9), 9, tab_of));
}

#[test]
fn closing_a_tab_ends_every_session_in_it_and_no_other() {
	let ended = |of| -> Vec<u64> {
		(0..10)
			.filter(|&id| ends(CloseTarget::Tab(of), id, tab_of))
			.collect()
	};
	assert_eq!(ended(1), [1, 2]);
	assert_eq!(ended(2), [1, 2], "named by any of its sessions");
	assert_eq!(ended(3), [3]);
	// A tab named by a session no tab holds ends nothing, not every
	// session outside the tabs.
	assert_eq!(ended(9), Vec::<u64>::new());
}

#[test]
fn a_name_with_nothing_printable_is_called_a_program() {
	assert_eq!(
		message(&["\x1b\x07"]).as_deref(),
		Some("A program is still running")
	);
	assert_eq!(
		message(&["  "]).as_deref(),
		Some("A program is still running")
	);
	let two = message(&["\n", "vim"]);
	assert_eq!(
		two.as_deref(),
		Some("A program and 1 more are still running")
	);
}

/// `popover` painted on a black window `size` big, with the bundled font.
fn painted(popover: &crate::render::popover::Popover, size: (usize, usize)) -> Vec<u32> {
	use crate::{
		fonts::{BUNDLED, Font},
		render::{glyphs::FontSet, painter::Painter, popover::draw_popover, scene::paint},
	};
	let mut fonts = FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new());
	let mut pixels = vec![0; size.0 * size.1];
	let mut painter = Painter::new(&mut pixels, size.0, size.1);
	paint(&mut painter, &mut fonts, |scene, fonts| {
		draw_popover(scene, fonts, popover)
	});
	pixels
}

/// The pixels of `pixels` (rows `width` wide) inside `rect`.
fn inside(pixels: &[u32], width: usize, (x, y, w, h): (usize, usize, usize, usize)) -> Vec<u32> {
	(y..y + h)
		.flat_map(|row| pixels[row * width + x..row * width + x + w].to_vec())
		.collect()
}

#[test]
fn the_prompt_draws_its_message_as_the_palette_draws_a_tab_name() {
	let theme = Theme::from_kind(ThemeKind::MacOsLight);
	for scale in [1, 2] {
		let (px, size) = (14.0 * scale as f32, (1000 * scale, 600 * scale));
		let text = "htop is still running";
		let prompt = close_prompt(place(scale), text, px, &theme);
		let renaming = command_palette(place(scale), (text, true), px, &theme);
		let (shown, palette) = (painted(&prompt, size), painted(&renaming, size));
		// The field, message and all, is the rename field's to the pixel.
		let field = prompt.field.rect;
		assert_eq!(
			inside(&shown, size.0, field),
			inside(&palette, size.0, field)
		);
		assert!(
			inside(&shown, size.0, field).contains(&theme.fg),
			"the message is drawn"
		);
		// Only the note under it differs, and it has text.
		let note = prompt.note.unwrap().0;
		assert_ne!(inside(&shown, size.0, note), inside(&palette, size.0, note));
		let outside: Vec<usize> = (0..shown.len())
			.filter(|&i| shown[i] != palette[i])
			.collect();
		let (x, y, w, h) = note;
		assert!(outside.iter().all(|&i| {
			let (col, row) = (i % size.0, i / size.0);
			(x..x + w).contains(&col) && (y..y + h).contains(&row)
		}));
	}
}

#[test]
fn the_prompt_fits_short_and_narrow_windows_as_the_palette_does() {
	let theme = Theme::from_kind(ThemeKind::Nord);
	let message = "a very long program name that runs past the panel's edge";
	for window in [(1000, 50), (300, 600), (120, 90), (40, 40)] {
		let place = Place {
			window,
			top: 44,
			scale: 1,
		};
		let prompt = close_prompt(place, message, 14.0, &theme);
		let palette = command_palette(place, ("", true), 14.0, &theme);
		assert_eq!(
			(prompt.rect, prompt.field.rect),
			(palette.rect, palette.field.rect)
		);
		let (x, _, width, _) = prompt.rect;
		assert!(
			x + width <= window.0,
			"inside the window across at {window:?}"
		);
		assert_eq!(
			prompt.field.text, message,
			"the field cuts the text, not the model"
		);
	}
}
