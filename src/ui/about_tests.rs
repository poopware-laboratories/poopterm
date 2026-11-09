//! Tests of the About page's lines: what they say, how they rank and where
//! they sit.
use super::{
	about::{FIRST_ROW, Line, Tone, about_lines, release_line},
	settings_layout::{FOOTER, MIN_HEIGHT},
};

fn lines() -> Vec<Line> {
	about_lines("26.10.8", "Giga Fly")
}

fn text(lines: &[Line]) -> String {
	lines
		.iter()
		.map(|line| line.text.as_str())
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn the_release_line_names_the_version_and_the_codename() {
	assert_eq!(
		release_line("26.10.8", "Giga Fly"),
		"Version 26.10.8 \"Giga Fly\""
	);
}

#[test]
fn the_page_opens_with_the_name_then_the_release_then_what_it_is() {
	let lines = lines();
	assert_eq!(lines[0].text, "poopterm");
	assert_eq!(lines[1].text, "Version 26.10.8 \"Giga Fly\"");
	assert_eq!(
		lines[2].text,
		"A small, native terminal emulator written in Rust."
	);
}

#[test]
fn the_name_is_the_largest_and_the_fine_print_the_smallest() {
	let lines = lines();
	let sizes: Vec<f32> = lines.iter().map(|line| line.size).collect();
	let name = sizes[0];
	let fine = sizes[sizes.len() - 1];
	assert!(sizes.iter().all(|&size| size <= name));
	assert!(sizes.iter().all(|&size| size >= fine));
	assert!(name > fine);
}

#[test]
fn only_the_name_and_what_it_is_are_drawn_in_full_strength() {
	let lines = lines();
	let strong: Vec<_> = lines
		.iter()
		.filter(|line| line.tone == Tone::Strong)
		.map(|line| line.text.as_str())
		.collect();
	assert_eq!(
		strong,
		[
			"poopterm",
			"A small, native terminal emulator written in Rust."
		]
	);
}

#[test]
fn rows_run_down_the_page_with_a_wider_gap_before_the_fine_print() {
	let rows: Vec<usize> = lines().iter().map(|line| line.row).collect();
	assert_eq!(rows[0], FIRST_ROW);
	assert!(rows.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
	let gaps: Vec<usize> = rows.windows(2).map(|pair| pair[1] - pair[0]).collect();
	// Name to release to tagline, then a break, then evenly spaced notices.
	let break_at = gaps
		.iter()
		.enumerate()
		.max_by_key(|&(_, gap)| gap)
		.unwrap()
		.0;
	assert_eq!(break_at, 2, "{gaps:?}");
	let notices = &gaps[3..];
	assert!(
		notices.windows(2).all(|pair| pair[0] == pair[1]),
		"{gaps:?}"
	);
}

#[test]
fn the_page_ends_above_the_footer_in_the_smallest_window() {
	let last = lines().last().unwrap().row;
	assert!(last + 26 <= MIN_HEIGHT - FOOTER, "{last}");
}

#[test]
fn it_names_the_license_cargo_declares() {
	assert_eq!(env!("CARGO_PKG_LICENSE"), "AGPL-3.0-or-later");
	let text = text(&lines());
	assert!(text.contains("Licensed under the GNU AGPL, version 3 or later."));
	assert!(!text.contains("MIT"));
}

#[test]
fn it_shows_the_notices_the_agpl_asks_of_interactive_programs() {
	// Section 0 and 5(d): a copyright notice, that there is no warranty,
	// and where to find the license.
	let text = text(&lines());
	assert!(text.contains("Copyright © 2026"), "{text}");
	assert!(text.contains("NO WARRANTY"), "{text}");
	assert!(text.contains("LICENSE"), "{text}");
	assert!(
		text.contains("github.com/poopware-laboratories/poopterm"),
		"{text}"
	);
}

#[test]
fn it_describes_the_project_as_it_is_built_today() {
	// The old line named winit and softbuffer, though Poopterm now draws
	// with its own renderers.
	let text = text(&lines());
	assert!(!text.contains("softbuffer"));
	assert!(text.contains("own renderers"), "{text}");
}

#[test]
fn no_line_repeats_another() {
	let lines = lines();
	for (index, line) in lines.iter().enumerate() {
		assert!(
			lines[index + 1..]
				.iter()
				.all(|other| other.text != line.text),
			"{}",
			line.text
		);
	}
}
