//! Which theme shows: the environment's, then the one configured for the
//! desktop's appearance, then `theme`, then Poop's own.
use crate::{
	theme::ThemeKind,
	theme_follow::{Appearance, Choices, chosen, of_window},
};

const BOTH: Choices = Choices {
	env: None,
	theme: Some("nord"),
	light: Some("catppuccin-latte"),
	dark: Some("dracula"),
};

#[test]
fn the_appearance_picks_its_own_theme() {
	assert_eq!(
		chosen(BOTH, Some(Appearance::Light)),
		ThemeKind::CatppuccinLatte
	);
	assert_eq!(chosen(BOTH, Some(Appearance::Dark)), ThemeKind::Dracula);
}

#[test]
fn until_the_desktop_says_or_without_a_theme_for_it_theme_shows() {
	assert_eq!(chosen(BOTH, None), ThemeKind::Nord, "no appearance yet");
	let light_only = Choices { dark: None, ..BOTH };
	assert_eq!(chosen(light_only, Some(Appearance::Dark)), ThemeKind::Nord);
	assert_eq!(
		chosen(light_only, Some(Appearance::Light)),
		ThemeKind::CatppuccinLatte
	);
}

#[test]
fn the_environment_wins_over_everything() {
	let env = Choices {
		env: Some("gruvbox-light"),
		..BOTH
	};
	for appearance in [None, Some(Appearance::Light), Some(Appearance::Dark)] {
		assert_eq!(
			chosen(env, appearance),
			ThemeKind::GruvboxLight,
			"{appearance:?}"
		);
	}
}

#[test]
fn names_that_are_no_theme_are_passed_over() {
	let typo = Choices {
		env: Some("nope"),
		light: Some("catppucin"),
		..BOTH
	};
	assert_eq!(
		chosen(typo, Some(Appearance::Light)),
		ThemeKind::Nord,
		"on to theme"
	);
	let all_wrong = Choices {
		env: Some("x"),
		theme: Some("y"),
		light: Some("z"),
		dark: Some("w"),
	};
	assert_eq!(chosen(all_wrong, Some(Appearance::Dark)), ThemeKind::Poop);
	assert_eq!(
		chosen(Choices::default(), Some(Appearance::Light)),
		ThemeKind::Poop
	);
}

#[test]
fn names_are_read_as_the_theme_key_reads_them() {
	// Quotes, case and the old names work as for `theme`.
	let quoted = Choices {
		light: Some("\"Light-Mode\""),
		dark: Some(" macos "),
		..Choices::default()
	};
	assert_eq!(
		chosen(quoted, Some(Appearance::Light)),
		ThemeKind::MacOsLight
	);
	assert_eq!(chosen(quoted, Some(Appearance::Dark)), ThemeKind::MacOsDark);
}

#[test]
fn winit_appearances_map_to_ours() {
	assert_eq!(of_window(winit::window::Theme::Light), Appearance::Light);
	assert_eq!(of_window(winit::window::Theme::Dark), Appearance::Dark);
}

/// The rule written out again: the first name that is a theme, in the
/// order environment, appearance's own key, `theme`.
fn reference(choices: Choices, appearance: Option<Appearance>) -> ThemeKind {
	let mut order = vec![choices.env];
	order.push(match appearance {
		Some(Appearance::Light) => choices.light,
		Some(Appearance::Dark) => choices.dark,
		None => None,
	});
	order.push(choices.theme);
	for name in order.into_iter().flatten() {
		if let Some(kind) = ThemeKind::parse(name) {
			return kind;
		}
	}
	ThemeKind::Poop
}

#[test]
fn every_combination_of_missing_wrong_and_right_names_follows_the_order() {
	// Each key unset, set to something that is no theme, or to its own theme.
	let values = |own: &'static str| [None, Some("no-such-theme"), Some(own)];
	let appearances = [None, Some(Appearance::Light), Some(Appearance::Dark)];
	let mut seen = Vec::new();
	for env in values("bark") {
		for theme in values("nord") {
			for light in values("rose-pine-dawn") {
				for dark in values("amoled-black") {
					for appearance in appearances {
						let choices = Choices {
							env,
							theme,
							light,
							dark,
						};
						let kind = chosen(choices, appearance);
						assert_eq!(
							kind,
							reference(choices, appearance),
							"{choices:?} {appearance:?}"
						);
						if !seen.contains(&kind) {
							seen.push(kind);
						}
					}
				}
			}
		}
	}
	// Every source wins somewhere, and so does the default.
	let expected = [
		ThemeKind::Bark,
		ThemeKind::Nord,
		ThemeKind::RosePineDawn,
		ThemeKind::AmoledBlack,
		ThemeKind::Poop,
	];
	assert_eq!(seen.len(), expected.len(), "{seen:?}");
	assert!(expected.iter().all(|kind| seen.contains(kind)), "{seen:?}");
}

/// A config choosing a theme per appearance, with color overrides.
const CONFIG: &str = "theme = \"nord\"\ntheme_light = \"catppuccin-latte\"\ntheme_dark = \"amoled-black\"\n\
	foreground = \"#123456\"\ncolor3 = \"#abcdef\"\n";

#[test]
fn a_followed_theme_takes_the_configured_colors_over_it() {
	use crate::theme::Theme;
	let light = Theme::from_config(CONFIG, None, Some(Appearance::Light));
	assert_eq!(light.kind, ThemeKind::CatppuccinLatte);
	assert_eq!(
		(light.fg, light.palette[3]),
		(0x12_3456, 0xab_cdef),
		"overrides on top"
	);
	let plain = Theme::from_kind(ThemeKind::CatppuccinLatte);
	assert_eq!(
		light.bg, plain.bg,
		"what is not overridden stays the theme's"
	);
	assert_eq!(light.palette[4], plain.palette[4]);
	// AMOLED Black keeps its own colors, as when chosen with `theme`.
	let dark = Theme::from_config(CONFIG, None, Some(Appearance::Dark));
	assert_eq!(dark.kind, ThemeKind::AmoledBlack);
	assert_eq!(dark.fg, Theme::from_kind(ThemeKind::AmoledBlack).fg);
}

#[test]
fn without_an_appearance_or_with_the_environment_set_the_old_rules_hold() {
	use crate::theme::Theme;
	assert_eq!(Theme::from_config(CONFIG, None, None).kind, ThemeKind::Nord);
	let env = Theme::from_config(CONFIG, Some("dracula"), Some(Appearance::Light));
	assert_eq!(
		(env.kind, env.fg),
		(ThemeKind::Dracula, 0x12_3456),
		"overrides apply to it too"
	);
	assert_eq!(
		Theme::from_config("", None, Some(Appearance::Dark)).kind,
		ThemeKind::Poop
	);
	// A config written before theme_light and theme_dark existed is read as before.
	let old = "theme = \"gruvbox-dark\"\n";
	for appearance in [None, Some(Appearance::Light), Some(Appearance::Dark)] {
		assert_eq!(
			Theme::from_config(old, None, appearance).kind,
			ThemeKind::GruvboxDark
		);
	}
}

#[test]
fn every_override_applies_and_values_that_are_no_color_are_ignored() {
	use crate::theme::Theme;
	let config = "theme_dark = \"gruvbox-dark\"\nbackground = \"#000001\"\nborder = \"#000002\"\n\
		color9 = \"#000003\"\nforeground = \"blue-ish\"\ncolor2 = \"#12\"\n";
	let theme = Theme::from_config(config, None, Some(Appearance::Dark));
	let plain = Theme::from_kind(ThemeKind::GruvboxDark);
	assert_eq!((theme.bg, theme.border), (0x00_0001, 0x00_0002));
	assert_eq!(
		theme.bright.map(|b| b[1]),
		Some(0x00_0003),
		"a bright color, 9"
	);
	assert_eq!(theme.fg, plain.fg, "not a color: the theme's own");
	assert_eq!(
		theme.palette[2], plain.palette[2],
		"too short: the theme's own"
	);
}
