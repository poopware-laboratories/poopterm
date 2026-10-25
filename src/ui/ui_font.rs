//! The proportional font window chrome is drawn in, and fitting chrome
//! text by its measured width.

/// UI families tried on Linux and BSD, most preferred first.
pub(crate) const LINUX_FAMILIES: [&str; 8] = [
	"Inter",
	"Adwaita Sans",
	"Cantarell",
	"Noto Sans",
	"Roboto",
	"Ubuntu",
	"DejaVu Sans",
	"Liberation Sans",
];

/// UI families tried on macOS, most preferred first.
pub(crate) const MACOS_FAMILIES: [&str; 5] = [
	"SF Pro Text",
	"SF Pro",
	"Helvetica Neue",
	"Helvetica",
	"Arial",
];

/// The UI families tried on macOS, or on every other platform.
pub(crate) fn platform_families(macos: bool) -> &'static [&'static str] {
	if macos {
		&MACOS_FAMILIES
	} else {
		&LINUX_FAMILIES
	}
}

/// The family chrome text is drawn in: the configured one when installed,
/// else the first installed entry of `families`, else `None` for the
/// bundled font.
pub(crate) fn choose_family<'a>(
	configured: Option<&'a str>,
	families: &[&'a str],
	installed: impl Fn(&str) -> bool,
) -> Option<&'a str> {
	let configured = configured
		.map(str::trim)
		.filter(|family| !family.is_empty());
	configured
		.into_iter()
		.chain(families.iter().copied())
		.find(|family| installed(family))
}

/// The width of `text` drawn with the given per-character advances.
pub(crate) fn text_width(text: &str, advance: impl FnMut(char) -> usize) -> usize {
	text.chars().map(advance).sum()
}

/// `text` whole if it fits in `max`, else its longest prefix that fits
/// beside a trailing ellipsis, without trailing spaces; empty when not even
/// the ellipsis fits.
pub(crate) fn ellipsize_to_width(
	text: &str,
	max: usize,
	mut advance: impl FnMut(char) -> usize,
) -> String {
	if text_width(text, &mut advance) <= max {
		return text.to_owned();
	}
	let Some(mut budget) = max.checked_sub(advance('…')) else {
		return String::new();
	};
	let mut end = 0;
	for (index, ch) in text.char_indices() {
		let width = advance(ch);
		if width > budget {
			break;
		}
		budget -= width;
		end = index + ch.len_utf8();
	}
	let mut fitted = text[..end].trim_end().to_owned();
	fitted.push('…');
	fitted
}

/// `text` whole if it fits in `max`, else its longest suffix that fits
/// after a leading ellipsis; empty when not even the ellipsis fits.
pub(crate) fn ellipsize_start_to_width(
	text: &str,
	max: usize,
	mut advance: impl FnMut(char) -> usize,
) -> String {
	if text_width(text, &mut advance) <= max {
		return text.to_owned();
	}
	let Some(mut budget) = max.checked_sub(advance('…')) else {
		return String::new();
	};
	let mut start = text.len();
	for (index, ch) in text.char_indices().rev() {
		let width = advance(ch);
		if width > budget {
			break;
		}
		budget -= width;
		start = index;
	}
	format!("…{}", &text[start..])
}

/// Fits a tab's title and status into `room` pixels: after a gap of two
/// spaces, the status gets at least half, or all the title leaves, and the
/// title the rest; all of it when the status cannot show a character.
pub(crate) fn fit_label(
	title: &str,
	status: Option<&str>,
	room: usize,
	mut advance: impl FnMut(char) -> usize,
) -> (String, String) {
	let gap = 2 * advance(' ');
	let title_width = text_width(title, &mut advance);
	let status_room = (room / 2).max(room.saturating_sub(title_width + gap));
	let shown = status.map_or_else(String::new, |status| {
		ellipsize_to_width(status, status_room, &mut advance)
	});
	if shown.is_empty() || shown == "…" {
		return (ellipsize_to_width(title, room, advance), String::new());
	}
	let title_room = room.saturating_sub(text_width(&shown, &mut advance) + gap);
	(ellipsize_to_width(title, title_room, advance), shown)
}
