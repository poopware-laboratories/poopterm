//! Tests of the UI font choice and of fitting chrome text by width.
use super::ui_font::{
	LINUX_FAMILIES, MACOS_FAMILIES, choose_family, ellipsize_start_to_width, ellipsize_to_width,
	fit_label, platform_families, text_width,
};

/// Advances of a toy proportional font: `i` is narrow, `m` and `…` wide.
fn advance(ch: char) -> usize {
	match ch {
		'i' | 'l' | ' ' => 3,
		'm' | 'w' => 12,
		'…' => 9,
		_ => 7,
	}
}

#[test]
fn the_configured_family_wins_when_it_is_installed() {
	// The font database matches families in any case.
	let installed =
		|family: &str| family.eq_ignore_ascii_case("Inter") || family.eq_ignore_ascii_case("Fira Sans");
	assert_eq!(
		choose_family(Some("fira sans"), &LINUX_FAMILIES, installed),
		Some("fira sans")
	);
}

#[test]
fn a_missing_configured_family_falls_back_to_the_list() {
	let installed = |family: &str| family == "Adwaita Sans";
	assert_eq!(
		choose_family(Some("Comic Neue"), &LINUX_FAMILIES, installed),
		Some("Adwaita Sans")
	);
	// A blank setting counts as none.
	assert_eq!(
		choose_family(Some("  "), &LINUX_FAMILIES, installed),
		Some("Adwaita Sans")
	);
}

#[test]
fn the_list_is_tried_in_order() {
	let installed = |family: &str| matches!(family, "DejaVu Sans" | "Noto Sans" | "Inter");
	assert_eq!(
		choose_family(None, &LINUX_FAMILIES, installed),
		Some("Inter")
	);
	let without_inter = |family: &str| matches!(family, "DejaVu Sans" | "Noto Sans");
	assert_eq!(
		choose_family(None, &LINUX_FAMILIES, without_inter),
		Some("Noto Sans")
	);
}

#[test]
fn no_installed_family_means_the_bundled_font() {
	assert_eq!(choose_family(None, &LINUX_FAMILIES, |_| false), None);
	assert_eq!(
		choose_family(Some("Inter"), &MACOS_FAMILIES, |_| false),
		None
	);
}

#[test]
fn macos_has_its_own_families() {
	assert_eq!(platform_families(true)[0], "SF Pro Text");
	assert_eq!(platform_families(false)[0], "Inter");
	assert!(!platform_families(true).contains(&"Cantarell"));
}

#[test]
fn every_platform_list_ends_in_a_widely_installed_family() {
	assert_eq!(LINUX_FAMILIES.last(), Some(&"Liberation Sans"));
	assert!(LINUX_FAMILIES.contains(&"DejaVu Sans"));
	assert_eq!(MACOS_FAMILIES.last(), Some(&"Arial"));
}

#[test]
fn widths_sum_each_advance() {
	assert_eq!(text_width("", advance), 0);
	assert_eq!(text_width("mil", advance), 12 + 3 + 3);
	assert_eq!(text_width("Shell", advance), 7 + 7 + 7 + 3 + 3);
}

#[test]
fn text_that_fits_is_kept_whole() {
	assert_eq!(ellipsize_to_width("Shell", 27, advance), "Shell");
	assert_eq!(ellipsize_to_width("Shell", 100, advance), "Shell");
}

#[test]
fn text_that_does_not_fit_ends_in_an_ellipsis_within_the_width() {
	// "Sh" is 14, the ellipsis 9: 23 fits in 24, "She…" (30) does not.
	assert_eq!(ellipsize_to_width("Shell", 24, advance), "Sh…");
	assert!(text_width(&ellipsize_to_width("Shell", 24, advance), advance) <= 24);
	// A prefix that fills the width exactly beside the ellipsis is kept.
	assert_eq!(ellipsize_to_width("Shell", 23, advance), "Sh…");
	assert_eq!(ellipsize_to_width("Shell", 22, advance), "S…");
	// Narrow characters let more of the text stay.
	// "illic" is 19 and "illici" 22, so only the first fits beside the ellipsis.
	assert_eq!(ellipsize_to_width("illicit mill", 30, advance), "illic…");
}

#[test]
fn trailing_spaces_are_dropped_before_the_ellipsis() {
	assert_eq!(ellipsize_to_width("ab cd", 26, advance), "ab…");
}

#[test]
fn widths_too_small_for_an_ellipsis_give_nothing() {
	assert_eq!(ellipsize_to_width("Shell", 8, advance), "");
	assert_eq!(ellipsize_to_width("Shell", 0, advance), "");
	assert_eq!(ellipsize_to_width("", 0, advance), "");
}

#[test]
fn only_the_ellipsis_shows_when_no_character_fits_beside_it() {
	assert_eq!(ellipsize_to_width("mmm", 10, advance), "…");
}

#[test]
fn a_label_without_status_is_only_ellipsized() {
	assert_eq!(
		fit_label("Shell", None, 24, advance),
		("Sh…".into(), String::new())
	);
	assert_eq!(
		fit_label("Shell", Some(""), 100, advance),
		("Shell".into(), String::new())
	);
}

#[test]
fn a_status_takes_the_room_the_title_leaves() {
	// "vim" is 22 and the gap two spaces (6), so "main" (29) fits whole.
	assert_eq!(
		fit_label("vim", Some("main"), 100, advance),
		("vim".into(), "main".into())
	);
	assert_eq!(
		fit_label("vim", Some("main"), 57, advance),
		("vim".into(), "main".into())
	);
	// More than half: of 70, the title leaves 42, so "runni…" (40) shows.
	assert_eq!(
		fit_label("vim", Some("running"), 70, advance),
		("vim".into(), "runni…".into())
	);
}

#[test]
fn a_short_status_leaves_the_title_the_rest() {
	assert_eq!(
		fit_label("Shell Shell", Some("ok"), 60, advance),
		("Shell…".into(), "ok".into())
	);
}

#[test]
fn a_long_title_leaves_the_status_half_the_room() {
	// Half of 60 is 30: "running" (45) shortens to "run…" and the title to
	// what the remaining 24 holds.
	assert_eq!(
		fit_label("Shell Shell", Some("running"), 60, advance),
		("Sh…".into(), "run…".into())
	);
}

#[test]
fn no_room_gives_empty_labels() {
	assert_eq!(
		fit_label("Shell", Some("running"), 0, advance),
		(String::new(), String::new())
	);
}

#[test]
fn a_configured_family_is_trimmed() {
	let installed = |family: &str| family == "Inter";
	assert_eq!(
		choose_family(Some("  Inter "), &LINUX_FAMILIES, installed),
		Some("Inter")
	);
}

#[test]
fn ellipsizing_cuts_between_characters_of_any_length() {
	// Each of these is 7 wide and 3 bytes long: 21 of 30 fits beside "…".
	assert_eq!(ellipsize_to_width("日本語のタブ", 30, advance), "日本語…");
	assert_eq!(ellipsize_to_width("🦀🦀🦀🦀🦀", 25, advance), "🦀🦀…");
}

#[test]
fn a_prefix_of_only_spaces_leaves_the_ellipsis_alone() {
	assert_eq!(ellipsize_to_width("    x", 12, advance), "…");
}

#[test]
fn ellipsized_text_fits_keeps_a_prefix_and_grows_with_the_width() {
	for text in [
		"Shell",
		"cargo build --release",
		"日本語 mix",
		"a  b  c",
		"mmmmmmmm",
	] {
		let mut previous = 0;
		for max in 0..=text_width(text, advance) + 4 {
			let fitted = ellipsize_to_width(text, max, advance);
			let width = text_width(&fitted, advance);
			assert!(width <= max, "{fitted:?} is {width} wide, over {max}");
			let kept = fitted.strip_suffix('…').unwrap_or(&fitted);
			assert!(
				text.starts_with(kept),
				"{fitted:?} is not a prefix of {text:?}"
			);
			assert_eq!(
				fitted == text,
				max >= text_width(text, advance),
				"{text:?} at {max}"
			);
			assert!(width >= previous, "{text:?} shrank to {fitted:?} at {max}");
			previous = width;
		}
	}
}

#[test]
fn fitted_labels_and_their_gap_stay_within_the_room() {
	for (title, status) in [
		("Shell", "running"),
		("cargo build", "ok"),
		("vim", "compiling"),
	] {
		for room in 0..=120 {
			let (label, shown) = fit_label(title, Some(status), room, advance);
			let mut width = text_width(&label, advance);
			if !shown.is_empty() {
				width += 6 + text_width(&shown, advance);
			}
			assert!(
				width <= room,
				"{label:?} {shown:?} is {width} wide, over {room}"
			);
		}
	}
}

#[test]
fn a_status_too_narrow_to_show_leaves_the_title_all_the_room() {
	// Half of 17 cannot hold the ellipsis, so the title takes all 17.
	assert_eq!(
		fit_label("Shell", Some("running"), 17, advance),
		("S…".into(), String::new())
	);
	// Half of 18 holds only the ellipsis, which says nothing on its own.
	assert_eq!(
		fit_label("Shell", Some("running"), 18, advance),
		("S…".into(), String::new())
	);
}

#[test]
fn combining_marks_stay_with_the_character_they_follow() {
	// The marks take no width, so a base that fits keeps them.
	let advance = |ch: char| if ch == '\u{301}' { 0 } else { advance(ch) };
	assert_eq!(
		ellipsize_to_width("e\u{301}e\u{301}e\u{301}x", 23, advance),
		"e\u{301}e\u{301}…"
	);
	assert_eq!(
		ellipsize_to_width("e\u{301}e\u{301}", 14, advance),
		"e\u{301}e\u{301}"
	);
}

#[test]
fn text_that_fits_keeps_its_start() {
	assert_eq!(ellipsize_start_to_width("Shell", 27, advance), "Shell");
	assert_eq!(ellipsize_start_to_width("", 0, advance), "");
}

#[test]
fn text_that_does_not_fit_keeps_its_end_after_an_ellipsis() {
	// "ll" is 6, the ellipsis 9: "…ll" (15) fits in 15, "…ell" (22) does not.
	assert_eq!(ellipsize_start_to_width("Shell", 15, advance), "…ll");
	assert_eq!(ellipsize_start_to_width("Shell", 21, advance), "…ll");
	assert_eq!(ellipsize_start_to_width("Shell", 22, advance), "…ell");
	assert!(text_width(&ellipsize_start_to_width("Shell", 24, advance), advance) <= 24);
	// Narrow characters at the end let more of them stay: "ill" is 9.
	assert_eq!(ellipsize_start_to_width("mmmmill", 18, advance), "…ill");
}

#[test]
fn the_start_ellipsis_keeps_whole_characters() {
	let kept = ellipsize_start_to_width("naïve façade", 40, advance);
	assert!(kept.starts_with('…'));
	assert!("naïve façade".ends_with(&kept[3..]), "{kept}");
	assert!(text_width(&kept, advance) <= 40);
	// One more character would not fit.
	let next = "naïve façade"
		.chars()
		.rev()
		.nth(kept.chars().count() - 1)
		.unwrap();
	assert!(text_width(&kept, advance) + advance(next) > 40);
}

#[test]
fn widths_too_small_for_a_start_ellipsis_give_nothing_or_the_ellipsis() {
	assert_eq!(ellipsize_start_to_width("Shell", 8, advance), "");
	assert_eq!(ellipsize_start_to_width("Shell", 0, advance), "");
	assert_eq!(ellipsize_start_to_width("mmm", 10, advance), "…");
}

#[test]
fn the_start_ellipsis_cuts_between_characters_of_any_length() {
	// Two, three and four bytes each, all 7 wide in the toy font.
	let text = "aé世🦀b";
	assert_eq!(ellipsize_start_to_width(text, 9 + 7, advance), "…b");
	assert_eq!(ellipsize_start_to_width(text, 9 + 14, advance), "…🦀b");
	assert_eq!(ellipsize_start_to_width(text, 9 + 21, advance), "…世🦀b");
	// One pixel short of the whole text, which is 35 wide.
	assert_eq!(ellipsize_start_to_width(text, 34, advance), "…世🦀b");
	assert_eq!(ellipsize_start_to_width(text, 35, advance), text);
}

#[test]
fn both_ellipses_agree_on_text_that_fits_and_stay_within_the_width() {
	for text in ["", "Shell", "naïve façade", "mmmmill", "aé世🦀b"] {
		for max in 0..80 {
			let start = ellipsize_start_to_width(text, max, advance);
			let end = ellipsize_to_width(text, max, advance);
			if text_width(text, advance) <= max {
				assert_eq!(
					(start.as_str(), end.as_str()),
					(text, text),
					"{text:?} in {max}"
				);
			} else {
				assert!(
					text_width(&start, advance) <= max,
					"{text:?} in {max}: {start:?}"
				);
				assert!(start.is_empty() || start.starts_with('…'), "{start:?}");
			}
		}
	}
}
