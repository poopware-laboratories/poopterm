//! Who draws the window's title bar and edges on Linux and FreeBSD: the
//! desktop, or the window itself, in its header bar.
use super::window_controls::ButtonLayout;
use crate::config::Preferences;

/// `decorations` in `config.toml`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DecorationPreference {
	/// The window draws its own where the compositor draws none.
	#[default]
	Auto,
	Client,
	Server,
}

impl DecorationPreference {
	pub(crate) fn parse(value: Option<String>) -> Self {
		match value.as_deref() {
			Some("client") => Self::Client,
			Some("server") => Self::Server,
			_ => Self::Auto,
		}
	}
}

/// Whether the window draws its own title bar and edges: when asked to,
/// or, left to choose, when the compositor offers to draw none (GNOME) and
/// the horizontal header bar is there to hold the buttons. `server_side`
/// is `None` when unknown, as on X11, where the window manager decorates.
pub(crate) fn client_side(preferences: &Preferences, server_side: Option<bool>) -> bool {
	let header = preferences.show_tabs && !preferences.vertical_tabs;
	match preferences.decorations {
		DecorationPreference::Client => true,
		DecorationPreference::Server => false,
		DecorationPreference::Auto => server_side == Some(false) && header,
	}
}

/// The buttons the header shows: the desktop's, once it has said which,
/// while the window draws its own title bar.
pub(crate) fn shown_buttons(client_side: bool, desktop: Option<&ButtonLayout>) -> ButtonLayout {
	desktop.filter(|_| client_side).cloned().unwrap_or_default()
}
