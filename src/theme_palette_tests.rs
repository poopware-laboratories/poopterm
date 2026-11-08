//! Themes that publish all 16 ANSI colors, their cursor and selection
//! colors, config overrides of colors 8 to 15, and every theme's credit.
use crate::{
	theme::{Theme, ThemeKind, bright_palette, readable_brights, xterm_color},
	ui::contrast::ratio,
};

/// Every theme the cycle visits, in order.
fn built_in() -> Vec<ThemeKind> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds
}

/// The 16 colors from draculatheme.com/spec and Gruvbox's palette.
const PUBLISHED: [(ThemeKind, u32, u32, [u32; 16]); 4] = [
	(
		ThemeKind::Dracula,
		0xF8F8F2,
		0x282A36,
		[
			0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C, 0xBD93F9, 0xFF79C6, 0x8BE9FD, 0xF8F8F2, 0x6272A4,
			0xFF6E6E, 0x69FF94, 0xFFFFA5, 0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF,
		],
	),
	(
		ThemeKind::Alucard,
		0x1F1F1F,
		0xFFFBEB,
		[
			0xFFFBEB, 0xCB3A2A, 0x14710A, 0x846E15, 0x644AC9, 0xA3144D, 0x036A96, 0x1F1F1F, 0x6C664B,
			0xD74C3D, 0x198D0C, 0x9E841A, 0x7862D0, 0xBF185A, 0x047FB4, 0x2C2B31,
		],
	),
	(
		ThemeKind::GruvboxDark,
		0xEBDBB2,
		0x282828,
		[
			0x282828, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0xA89984, 0x928374,
			0xFB4934, 0xB8BB26, 0xFABD2F, 0x83A598, 0xD3869B, 0x8EC07C, 0xEBDBB2,
		],
	),
	(
		ThemeKind::GruvboxLight,
		0x3C3836,
		0xFBF1C7,
		[
			0xFBF1C7, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0x7C6F64, 0x928374,
			0x9D0006, 0x79740E, 0xB57614, 0x076678, 0x8F3F71, 0x427B58, 0x3C3836,
		],
	),
];

#[test]
fn new_themes_match_their_published_16_colors() {
	for (kind, fg, bg, colors) in PUBLISHED {
		let theme = Theme::from_kind(kind);
		assert_eq!(theme.kind, kind);
		assert_eq!((theme.fg, theme.bg), (fg, bg), "{}", kind.name());
		assert_eq!(theme.palette, colors[..8], "{}", kind.name());
		assert_eq!(theme.bright, Some(colors[8..].try_into().unwrap()));
	}
}

#[test]
fn dark_themes_draw_their_published_brights_unchanged() {
	for kind in [ThemeKind::Dracula, ThemeKind::GruvboxDark] {
		let theme = Theme::from_kind(kind);
		let bright = theme.bright.unwrap();
		assert_eq!(theme.bright_colors(), bright, "{}", kind.name());
		assert_eq!(xterm_color(9, theme.palette, bright), bright[1]);
	}
}

#[test]
fn light_themes_darken_published_brights_only_until_they_read() {
	for kind in [
		ThemeKind::Alucard,
		ThemeKind::GruvboxLight,
		ThemeKind::CatppuccinLatte,
	] {
		let theme = Theme::from_kind(kind);
		let published = theme.bright.unwrap();
		for (index, (color, original)) in theme.bright_colors().into_iter().zip(published).enumerate() {
			let contrast = ratio(color, theme.bg);
			assert!(contrast >= 4.5, "{} {index}: {contrast:.2}", kind.name());
			if color != original {
				assert!(ratio(original, theme.bg) < 4.5, "{} {index}", kind.name());
				assert!(contrast < 4.7, "{} {index} darker than needed", kind.name());
			}
		}
	}
	// Gruvbox Light's faded red already reads and is kept exactly.
	assert_eq!(
		Theme::from_kind(ThemeKind::GruvboxLight).bright_colors()[1],
		0x9D0006
	);
}

#[test]
fn readable_brights_on_xterms_colors_are_the_bright_palette() {
	const XTERM: [u32; 8] = [
		0x808080, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
	];
	for (fg, bg) in [
		(0x000000, 0xFFFFFF),
		(0xFFFFFF, 0x000000),
		(0x1D1D1F, 0xF5F5F7),
	] {
		assert_eq!(readable_brights(XTERM, fg, bg), bright_palette(fg, bg));
	}
	// A dark background keeps any brights as they are, even unreadable ones.
	assert_eq!(
		readable_brights([0x000001; 8], 0xFFFFFF, 0x000000),
		[0x000001; 8]
	);
}

#[test]
fn catppuccin_follows_its_style_guide_for_the_16_colors_and_cursor() {
	// (kind, color 7 subtext 0 or surface 2, color 8, color 15, rosewater)
	let flavors = [
		(
			ThemeKind::CatppuccinLatte,
			0xACB0BE,
			0x6C6F85,
			0xBCC0CC,
			0xDC8A78,
		),
		(
			ThemeKind::CatppuccinFrappe,
			0xA5ADCE,
			0x626880,
			0xB5BFE2,
			0xF2D5CF,
		),
		(
			ThemeKind::CatppuccinMacchiato,
			0xA5ADCB,
			0x5B6078,
			0xB8C0E0,
			0xF4DBD6,
		),
		(
			ThemeKind::CatppuccinMocha,
			0xA6ADC8,
			0x585B70,
			0xBAC2DE,
			0xF5E0DC,
		),
	];
	for (kind, white, bright_black, bright_white, cursor) in flavors {
		let theme = Theme::from_kind(kind);
		let bright = theme.bright.unwrap();
		assert_eq!(theme.palette[7], white, "{}", kind.name());
		assert_eq!((bright[0], bright[7]), (bright_black, bright_white));
		// Bright red to bright cyan repeat the normal colors.
		assert_eq!(bright[1..7], theme.palette[1..7], "{}", kind.name());
		assert_eq!(theme.cursor, Some(cursor));
		assert_eq!(theme.selection, None);
	}
}

#[test]
fn cursor_and_selection_come_only_from_themes_that_publish_them() {
	let nord = Theme::from_kind(ThemeKind::Nord);
	assert_eq!(nord.cursor, Some(0xD8DEE9));
	assert_eq!(nord.selection, Some((0xD8DEE9, 0x434C5E)));
	let dracula = Theme::from_kind(ThemeKind::Dracula);
	assert_eq!(
		(dracula.cursor, dracula.selection),
		(None, Some((0xF8F8F2, 0x44475A)))
	);
	let alucard = Theme::from_kind(ThemeKind::Alucard);
	assert_eq!(alucard.selection, Some((0x1F1F1F, 0xCFCFDE)));
	for kind in [
		ThemeKind::MacOsDark,
		ThemeKind::RosePine,
		ThemeKind::Bark,
		ThemeKind::GruvboxDark,
		ThemeKind::Poop,
	] {
		let theme = Theme::from_kind(kind);
		assert_eq!(
			(theme.cursor, theme.selection),
			(None, None),
			"{}",
			kind.name()
		);
	}
}

#[test]
fn every_theme_selection_reads() {
	for kind in built_in() {
		if let Some((fg, bg)) = Theme::from_kind(kind).selection {
			assert!(
				ratio(fg, bg) >= 4.5,
				"{}: {:.2}",
				kind.name(),
				ratio(fg, bg)
			);
		}
	}
}

#[test]
fn new_theme_names_and_aliases_parse_and_round_trip() {
	for (name, kind) in [
		("dracula", ThemeKind::Dracula),
		("alucard", ThemeKind::Alucard),
		("dracula-light", ThemeKind::Alucard),
		("gruvbox", ThemeKind::GruvboxDark),
		("gruvbox-dark", ThemeKind::GruvboxDark),
		(" \"Gruvbox-Light\" ", ThemeKind::GruvboxLight),
	] {
		assert_eq!(ThemeKind::parse(name), Some(kind), "{name}");
		assert_eq!(ThemeKind::parse(kind.name()), Some(kind));
	}
	for name in ["solarized", "gruvbox-medium", "dracula-dark"] {
		assert_eq!(ThemeKind::parse(name), None, "{name}");
	}
}

#[test]
fn old_config_theme_names_still_parse() {
	for (name, kind) in [
		("macos", ThemeKind::MacOsDark),
		("mac", ThemeKind::MacOsDark),
		("light", ThemeKind::MacOsLight),
		("light mode", ThemeKind::MacOsLight),
		("rosepine", ThemeKind::RosePine),
		("catppuccin", ThemeKind::CatppuccinMocha),
		("amoled", ThemeKind::AmoledBlack),
	] {
		assert_eq!(ThemeKind::parse(name), Some(kind), "{name}");
	}
}

#[test]
fn the_cycle_reaches_the_new_themes_between_amoled_and_poop() {
	assert_eq!(ThemeKind::AmoledBlack.next(), ThemeKind::Dracula);
	assert_eq!(ThemeKind::Dracula.next(), ThemeKind::Alucard);
	assert_eq!(ThemeKind::Alucard.next(), ThemeKind::GruvboxDark);
	assert_eq!(ThemeKind::GruvboxDark.next(), ThemeKind::GruvboxLight);
	assert_eq!(ThemeKind::GruvboxLight.next(), ThemeKind::Poop);
	assert_eq!(built_in().len(), 17);
}

#[test]
fn config_colors_8_to_15_set_the_brights() {
	let mut theme = Theme::from_kind(ThemeKind::Nord);
	assert_eq!(theme.bright, None);
	let config = |index: usize| match index {
		1 => Some(0x111111),
		9 => Some(0x999999),
		15 => Some(0xFAFAFA),
		_ => None,
	};
	theme.override_colors(config);
	assert_eq!(theme.palette[1], 0x111111);
	let bright = theme.bright_colors();
	assert_eq!((bright[1], bright[7]), (0x999999, 0xFAFAFA));
	// The other brights stay xterm's.
	assert_eq!((bright[0], bright[2]), (0x808080, 0x55FF55));
}

#[test]
fn config_brights_replace_a_themes_published_ones_one_by_one() {
	let mut theme = Theme::from_kind(ThemeKind::Dracula);
	theme.override_colors(|index| (index == 12).then_some(0x0000FF));
	let bright = theme.bright.unwrap();
	assert_eq!(bright[4], 0x0000FF);
	assert_eq!(bright[3], 0xFFFFA5);
}

#[test]
fn no_config_colors_leave_the_theme_untouched() {
	for kind in built_in() {
		let mut theme = Theme::from_kind(kind);
		let (palette, bright) = (theme.palette, theme.bright);
		theme.override_colors(|_| None);
		assert_eq!(
			(theme.palette, theme.bright),
			(palette, bright),
			"{}",
			kind.name()
		);
	}
}

#[test]
fn every_theme_is_credited_with_a_license_that_allows_it() {
	for kind in built_in().into_iter().chain([ThemeKind::Custom]) {
		let credit = kind.credit();
		assert!(!credit.author.is_empty(), "{}", kind.name());
		assert!(credit.source.contains('.'), "{}", kind.name());
		assert!(
			["MIT", "AGPL-3.0-or-later"].contains(&credit.license),
			"{}: {}",
			kind.name(),
			credit.license
		);
	}
}

#[test]
fn third_party_themes_name_their_authors_and_mit_license() {
	for (kind, author, source) in [
		(
			ThemeKind::Nord,
			"Arctic Ice Studio & Sven Greb",
			"nordtheme.com",
		),
		(ThemeKind::RosePineDawn, "Rosé Pine", "rosepinetheme.com"),
		(ThemeKind::CatppuccinFrappe, "Catppuccin", "catppuccin.com"),
		(
			ThemeKind::Dracula,
			"Zeno Rocha and Dracula Theme",
			"draculatheme.com",
		),
		(
			ThemeKind::Alucard,
			"Zeno Rocha and Dracula Theme",
			"draculatheme.com",
		),
		(
			ThemeKind::GruvboxLight,
			"Pavel Pertsev",
			"github.com/morhetz/gruvbox",
		),
		(
			ThemeKind::Bark,
			"Chris Kempson (base16)",
			"github.com/chriskempson/base16",
		),
	] {
		let credit = kind.credit();
		assert_eq!(
			(credit.author, credit.source, credit.license),
			(author, source, "MIT")
		);
	}
	for kind in [ThemeKind::Poop, ThemeKind::AmoledBlack, ThemeKind::Custom] {
		assert_eq!(kind.credit().license, "AGPL-3.0-or-later");
	}
}

#[test]
fn a_credit_reads_as_one_line() {
	assert_eq!(
		ThemeKind::Dracula.credit().line(),
		"by Zeno Rocha and Dracula Theme · MIT · draculatheme.com"
	);
}
