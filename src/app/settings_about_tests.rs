//! Tests of the About page as drawn: every line fits and is centered in
//! every window size, they do not run into each other, and the quiet text
//! stays readable on every theme.
use super::{
	settings_pen_tests::{record, texts},
	settings_view::{MIN_SIZE, compose_about},
};
use crate::{
	render::scene::Command,
	theme::{Theme, ThemeKind},
	ui::{
		about::about_lines,
		chrome_colors::chrome_colors,
		contrast::{TEXT, ratio},
	},
};

const SIZES: [(usize, usize); 3] = [MIN_SIZE, (760, 640), (1300, 900)];

/// The text commands of the page drawn at `scale` in a window of `size`.
fn drawn(scale: usize, size: (usize, usize)) -> Vec<Command> {
	record(scale, |pen, p, _| {
		(p.width, p.height) = (size.0 * scale, size.1 * scale);
		compose_about(pen, p, (0xffffff, 0x808080))
	})
}

fn every_theme() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

#[test]
fn the_page_draws_every_line_the_model_lists_in_order() {
	let expected: Vec<_> = about_lines(env!("CARGO_PKG_VERSION"), env!("POOPTERM_CODENAME"))
		.into_iter()
		.map(|line| line.text)
		.collect();
	for scale in [1, 2] {
		let shown: Vec<_> = texts(&drawn(scale, MIN_SIZE))
			.into_iter()
			.map(|(text, _)| text)
			.collect();
		assert_eq!(shown, expected, "{scale}x");
	}
}

#[test]
fn each_line_sits_on_its_row_at_every_scale() {
	let lines = about_lines(env!("CARGO_PKG_VERSION"), env!("POOPTERM_CODENAME"));
	for scale in [1, 2] {
		let shown = texts(&drawn(scale, MIN_SIZE));
		for (line, (_, at)) in lines.iter().zip(&shown) {
			assert_eq!(at.1, line.row * scale, "{} at {scale}x", line.text);
		}
	}
}

#[test]
fn every_line_fits_inside_the_window_with_a_margin() {
	for scale in [1, 2] {
		let mut widest = 0;
		let _ = record(scale, |pen, _, _| {
			for line in about_lines(env!("CARGO_PKG_VERSION"), env!("POOPTERM_CODENAME")) {
				widest = widest.max(pen.width(&line.text, line.size));
			}
		});
		// The smallest window leaves the longest line the margin each side.
		assert!(
			widest + 2 * 24 * scale <= MIN_SIZE.0 * scale,
			"{widest} at {scale}x"
		);
	}
}

#[test]
fn every_line_is_centered_in_every_window_size() {
	for scale in [1, 2] {
		for size in SIZES {
			let commands = drawn(scale, size);
			let starts: Vec<_> = texts(&commands).into_iter().map(|(_, at)| at.0).collect();
			// A window twice as wide moves every line right by half the difference.
			let wider = drawn(scale, (size.0 + 200, size.1));
			let moved: Vec<_> = texts(&wider).into_iter().map(|(_, at)| at.0).collect();
			for (before, after) in starts.iter().zip(&moved) {
				assert_eq!(after - before, 100 * scale, "{size:?} at {scale}x");
			}
		}
	}
}

#[test]
fn lines_never_share_a_row_so_none_overprints_another() {
	let rows: Vec<_> = texts(&drawn(1, MIN_SIZE))
		.into_iter()
		.map(|(_, at)| at.1)
		.collect();
	let mut sorted = rows.clone();
	sorted.sort_unstable();
	sorted.dedup();
	assert_eq!(sorted.len(), rows.len());
	assert_eq!(sorted, rows, "rows already run top to bottom");
}

#[test]
fn the_quiet_text_reads_on_the_background_of_every_theme() {
	for theme in every_theme() {
		let muted = chrome_colors(&theme).muted;
		let contrast = ratio(muted, theme.bg);
		assert!(contrast >= TEXT, "{:?}: {contrast:.2}", theme.kind);
	}
}

#[test]
fn the_quiet_text_is_quieter_than_the_strong_text_on_every_theme() {
	for theme in every_theme() {
		let muted = chrome_colors(&theme).muted;
		assert!(
			ratio(muted, theme.bg) <= ratio(theme.fg, theme.bg),
			"{:?}",
			theme.kind
		);
	}
}

#[test]
fn strong_and_quiet_lines_take_the_colors_they_are_given() {
	let commands = drawn(1, MIN_SIZE);
	let colors: Vec<_> = commands
		.iter()
		.filter_map(|command| match command {
			Command::Text { color, .. } => Some(*color),
			_ => None,
		})
		.collect();
	assert_eq!(colors[0], 0xffffff, "the name");
	assert_eq!(colors[1], 0x808080, "the release");
	assert_eq!(colors[2], 0xffffff, "what it is");
	assert!(
		colors[3..].iter().all(|&color| color == 0x808080),
		"the notices"
	);
}
