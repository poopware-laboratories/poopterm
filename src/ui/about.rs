//! The About page's lines.

/// Where the name sits, in logical pixels from the window's top.
pub(crate) const FIRST_ROW: usize = 132;
const NAME_SIZE: f32 = 1.5;
const BODY_SIZE: f32 = 0.82;
const FINE_SIZE: f32 = 0.68;
/// Name to release, release to tagline, tagline to the first notice, and
/// notice to notice.
const AFTER_NAME: usize = 36;
const AFTER_RELEASE: usize = 30;
const BEFORE_NOTICES: usize = 48;
const NOTICE_STRIDE: usize = 26;

/// The AGPL asks an interactive program to show its copyright, that there is
/// no warranty, and where the license is.
const NOTICES: [&str; 5] = [
	"Copyright © 2026 poopware-labs and contributors",
	"Licensed under the GNU AGPL, version 3 or later.",
	"Free software with ABSOLUTELY NO WARRANTY; see LICENSE.",
	"Source: github.com/poopware-laboratories/poopterm",
	"Built in Rust with its own renderers, font engine and parser.",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
	Strong,
	Muted,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
	pub text: String,
	pub row: usize,
	pub size: f32,
	pub tone: Tone,
}

pub(crate) fn release_line(version: &str, codename: &str) -> String {
	format!("Version {version} \"{codename}\"")
}

/// The page's lines top to bottom: the name, the release, what Poopterm is,
/// then the notices in small, quiet text.
pub(crate) fn about_lines(version: &str, codename: &str) -> Vec<Line> {
	let line = |text: &str, row, size, tone| Line {
		text: text.to_owned(),
		row,
		size,
		tone,
	};
	let release = FIRST_ROW + AFTER_NAME;
	let tagline = release + AFTER_RELEASE;
	let first = tagline + BEFORE_NOTICES;
	let mut lines = vec![
		line("poopterm", FIRST_ROW, NAME_SIZE, Tone::Strong),
		line(
			&release_line(version, codename),
			release,
			BODY_SIZE,
			Tone::Muted,
		),
		line(
			"A small, native terminal emulator written in Rust.",
			tagline,
			BODY_SIZE,
			Tone::Strong,
		),
	];
	for (index, notice) in NOTICES.into_iter().enumerate() {
		let row = first + index * NOTICE_STRIDE;
		lines.push(line(notice, row, FINE_SIZE, Tone::Muted));
	}
	lines
}
