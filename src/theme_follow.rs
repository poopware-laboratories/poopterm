//! Which theme to show: `POOPTERM_THEME`, else `theme_light` or
//! `theme_dark` for the desktop's appearance when one is set for it, else
//! `theme`, else Poop's own.
use crate::theme::ThemeKind;

/// Whether the desktop asks for light or dark windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Appearance {
	Light,
	Dark,
}

/// The theme names that decide, as the environment and `config.toml`
/// give them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Choices<'a> {
	pub env: Option<&'a str>,
	pub theme: Option<&'a str>,
	pub light: Option<&'a str>,
	pub dark: Option<&'a str>,
}

/// The theme for `appearance` (`None` while the desktop has not said);
/// names that are no theme are passed over.
pub(crate) fn chosen(choices: Choices, appearance: Option<Appearance>) -> ThemeKind {
	let follow = match appearance {
		Some(Appearance::Light) => choices.light,
		Some(Appearance::Dark) => choices.dark,
		None => None,
	};
	[choices.env, follow, choices.theme]
		.into_iter()
		.flatten()
		.find_map(ThemeKind::parse)
		.unwrap_or(ThemeKind::Poop)
}

/// The appearance winit reports for a window.
pub(crate) fn of_window(theme: winit::window::Theme) -> Appearance {
	match theme {
		winit::window::Theme::Light => Appearance::Light,
		winit::window::Theme::Dark => Appearance::Dark,
	}
}
