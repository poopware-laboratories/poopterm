//! The `window_frame` key: whether the window keeps its frame (title bar,
//! window buttons, rounded corners) or is just the terminal in a square.

/// `window_frame` in `config.toml`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WindowFrame {
	/// The platform's own frame: macOS title bar with traffic lights, or
	/// whatever `decorations` chooses elsewhere.
	#[default]
	Normal,
	/// No title bar, no window buttons, square corners (`"none"`).
	Frameless,
}

impl WindowFrame {
	/// `"none"` squares the window; anything else keeps the frame.
	pub(crate) fn parse(value: Option<String>) -> Self {
		match value.as_deref() {
			Some("none") => Self::Frameless,
			_ => Self::Normal,
		}
	}

	/// The value written to `config.toml`.
	pub(crate) fn name(self) -> &'static str {
		match self {
			Self::Normal => "normal",
			Self::Frameless => "none",
		}
	}

	pub(crate) fn is_frameless(self) -> bool {
		self == Self::Frameless
	}
}

/// Whether the window has a title bar the tab strip sits in: only on
/// macOS, and only while the frame is kept.
pub(crate) fn has_titlebar(frame: WindowFrame, macos: bool) -> bool {
	macos && !frame.is_frameless()
}

/// The unscaled width the traffic lights take left of the tabs, which is
/// none without a title bar.
pub(crate) fn traffic_lights_reserve(frame: WindowFrame, macos: bool) -> usize {
	if has_titlebar(frame, macos) { 80 } else { 0 }
}

/// The first pixel row of the horizontal tab bar: centred in the title bar
/// when there is one, else flush with the window's top edge, above the
/// padding.
pub(crate) fn tab_bar_top(titlebar_height: usize, tab_height: usize) -> usize {
	titlebar_height.saturating_sub(tab_height) / 2
}

/// Whether a press on the empty header drags the window ourselves: when
/// the window draws its own edges, or has no native title bar to do it.
pub(crate) fn drags_bar(client_side: bool, frame: WindowFrame) -> bool {
	client_side || frame.is_frameless()
}

/// Whether the window may be asked if it is maximized. On macOS winit
/// answers for a borderless window by briefly giving it a title bar, which
/// moves its frame, so a square window is never asked: a tiling window
/// manager would fight the shaking, and clicks would lose their release.
pub(crate) fn may_ask_if_maximized(frame: WindowFrame, macos: bool) -> bool {
	!(macos && frame.is_frameless())
}
