//! The key bar: its layout, taps, and the Ctrl that holds for one key.
use super::*;

fn ctrl() -> Modifiers {
	Modifiers {
		ctrl: true,
		..Modifiers::default()
	}
}

#[test]
fn the_keys_share_the_width_left_to_right_inside_the_bar() {
	let rects = key_rects((100, 2000), 1206, 3);
	assert_eq!(rects.len(), KEYS.len());
	let order: Vec<BarKey> = rects.iter().map(|&(_, key)| key).collect();
	assert_eq!(order, KEYS.map(|(key, _)| key));
	for pair in rects.windows(2) {
		let ((x0, _, w0, _), _) = pair[0];
		let ((x1, _, _, _), _) = pair[1];
		assert_eq!(x1, x0 + w0 + 18, "a gap of 6 points between keys");
	}
	let ((x, y, w, h), _) = rects[0];
	assert_eq!((x, y, h), (100 + 18, 2000 + 18, (44 - 12) * 3));
	let ((last_x, _, last_w, _), _) = rects[KEYS.len() - 1];
	assert!(last_x + last_w <= 100 + 1206, "inside the bar");
	assert!(w > 100, "wide enough to tap: {w}");
}

#[test]
fn a_bar_too_narrow_for_its_gaps_has_keys_of_no_width_and_no_panic() {
	let rects = key_rects((0, 0), 10, 3);
	assert!(rects.iter().all(|&((_, _, w, _), _)| w == 0));
	assert_eq!(key_at((5, 20), &rects), None);
}

#[test]
fn a_tap_finds_the_key_under_it_and_none_between() {
	let rects = key_rects((0, 0), 1206, 3);
	for &((x, y, w, h), key) in &rects {
		assert_eq!(key_at((x + w / 2, y + h / 2), &rects), Some(key));
		assert_eq!(key_at((x, y), &rects), Some(key), "its corner");
	}
	let ((x, y, w, _), _) = rects[0];
	assert_eq!(key_at((x + w + 1, y + 5), &rects), None, "the gap after it");
	assert_eq!(key_at((x + 5, 0), &rects), None, "above the keys");
}

#[test]
fn the_named_keys_send_what_a_keyboard_would() {
	let mut bar = KeyBar::default();
	let cases = [
		(BarKey::Esc, NamedKey::Escape),
		(BarKey::Tab, NamedKey::Tab),
		(BarKey::Left, NamedKey::ArrowLeft),
		(BarKey::Down, NamedKey::ArrowDown),
		(BarKey::Up, NamedKey::ArrowUp),
		(BarKey::Right, NamedKey::ArrowRight),
	];
	for (key, named) in cases {
		assert_eq!(
			bar.press(key),
			Press::Send(Key::Named(named), Modifiers::default())
		);
	}
}

#[test]
fn ctrl_holds_for_the_next_key_only() {
	let mut bar = KeyBar::default();
	assert_eq!(bar.press(BarKey::Ctrl), Press::Latched(true));
	assert!(bar.ctrl());
	let c = Key::Character("c".into());
	assert_eq!(bar.take(&c, Modifiers::default()), ctrl(), "ctrl+c");
	assert!(!bar.ctrl(), "spent");
	assert_eq!(bar.take(&c, Modifiers::default()), Modifiers::default());
}

#[test]
fn ctrl_pressed_twice_lets_go() {
	let mut bar = KeyBar::default();
	bar.press(BarKey::Ctrl);
	assert_eq!(bar.press(BarKey::Ctrl), Press::Latched(false));
	assert_eq!(
		bar.take(&Key::Character("c".into()), Modifiers::default()),
		Modifiers::default()
	);
}

#[test]
fn ctrl_then_an_arrow_sends_ctrl_with_it() {
	let mut bar = KeyBar::default();
	bar.press(BarKey::Ctrl);
	assert_eq!(
		bar.press(BarKey::Left),
		Press::Send(Key::Named(NamedKey::ArrowLeft), ctrl())
	);
	assert!(!bar.ctrl());
}

#[test]
fn ctrl_then_a_capital_is_ctrl_shift_so_ctrl_p_opens_the_palette() {
	let mut bar = KeyBar::default();
	bar.press(BarKey::Ctrl);
	let shifted = bar.take(&Key::Character("P".into()), Modifiers::default());
	assert_eq!(
		shifted,
		Modifiers {
			ctrl: true,
			shift: true,
			..Modifiers::default()
		}
	);
	let palette = crate::input::shortcuts::Shortcut::parse("ctrl+shift+p").unwrap();
	assert!(palette.matches(&Key::Character("P".into()), shifted));
	bar.press(BarKey::Ctrl);
	let lower = bar.take(&Key::Character("p".into()), Modifiers::default());
	assert!(!lower.shift, "a small letter is plain ctrl");
	for word in ["Pa", "É", "1"] {
		bar.press(BarKey::Ctrl);
		assert!(
			!bar
				.take(&Key::Character(word.into()), Modifiers::default())
				.shift,
			"{word}"
		);
	}
}

#[test]
fn modifiers_already_held_are_kept() {
	let mut bar = KeyBar::default();
	bar.press(BarKey::Ctrl);
	let alt = Modifiers {
		alt: true,
		..Modifiers::default()
	};
	let held = bar.take(&Key::Character("x".into()), alt);
	assert!(held.alt && held.ctrl);
}

#[test]
fn the_palette_key_opens_it_and_lets_ctrl_go() {
	let mut bar = KeyBar::default();
	bar.press(BarKey::Ctrl);
	assert_eq!(bar.press(BarKey::Palette), Press::Palette);
	assert!(!bar.ctrl());
}

#[test]
fn the_bar_shows_only_on_ios_with_the_keyboard_up() {
	assert!(
		shown(true, 1000, 102),
		"the keyboard covers more than the home indicator"
	);
	assert!(
		!shown(true, 102, 102),
		"only the home indicator: keyboard away"
	);
	assert!(
		!shown(false, 1000, 102),
		"a desktop has a keyboard of its own"
	);
	assert!(!shown(true, 0, 0));
}

#[test]
fn every_key_has_a_short_label() {
	for (_, label) in KEYS {
		assert!((1..=4).contains(&label.chars().count()), "{label}");
	}
}

#[test]
fn the_bar_adds_its_height_to_the_bottom_only_while_it_shows() {
	assert_eq!(bottom_with_bar(true, 1000, 102, 3), 1000 + 44 * 3);
	assert_eq!(bottom_with_bar(true, 102, 102, 3), 102, "keyboard away");
	assert_eq!(
		bottom_with_bar(false, 1000, 102, 2),
		1000,
		"not on a desktop"
	);
}

#[test]
fn the_strip_sits_right_above_the_keyboard_and_inside_the_sides() {
	// An iPhone on its side: 2622×1206, the keyboard 600 pixels tall.
	let rect = strip((2622, 1206), 600, (186, 186), 3);
	assert_eq!(rect, (186, 1206 - 600 - 132, 2622 - 372, 132));
	let portrait = strip((1206, 2622), 1000, (0, 0), 3);
	assert_eq!(
		portrait.1 + portrait.3,
		2622 - 1000,
		"its bottom is the keyboard's top"
	);
}

#[test]
fn a_strip_in_a_window_too_small_stays_inside_it() {
	let (x, y, w, h) = strip((100, 50), 400, (80, 80), 3);
	assert_eq!((x, y, w), (80, 0, 0));
	assert!(h <= 50);
}
