//! Who made each built-in theme, where its palette is published, and the
//! license it is shared under. Only themes whose licenses allow it are built in.
use crate::theme::ThemeKind;

/// Credit for a theme's colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Credit {
	pub(crate) author: &'static str,
	/// Where the palette is published.
	pub(crate) source: &'static str,
	/// SPDX identifier of the palette's license.
	pub(crate) license: &'static str,
}

const POOPTERM: Credit = Credit {
	author: "Poopterm contributors",
	source: "github.com/poopware-laboratories/poopterm",
	license: "AGPL-3.0-or-later",
};

impl ThemeKind {
	pub(crate) fn credit(self) -> Credit {
		let (author, source, license) = match self {
			Self::Nord => ("Arctic Ice Studio & Sven Greb", "nordtheme.com", "MIT"),
			Self::RosePine | Self::RosePineMoon | Self::RosePineDawn => {
				("Rosé Pine", "rosepinetheme.com", "MIT")
			}
			Self::CatppuccinLatte
			| Self::CatppuccinFrappe
			| Self::CatppuccinMacchiato
			| Self::CatppuccinMocha => ("Catppuccin", "catppuccin.com", "MIT"),
			Self::Bark => (
				"Chris Kempson (base16)",
				"github.com/chriskempson/base16",
				"MIT",
			),
			Self::Dracula | Self::Alucard => ("Zeno Rocha and Dracula Theme", "draculatheme.com", "MIT"),
			Self::GruvboxDark | Self::GruvboxLight => {
				("Pavel Pertsev", "github.com/morhetz/gruvbox", "MIT")
			}
			Self::MacOsDark | Self::MacOsLight | Self::AmoledBlack | Self::Poop | Self::Custom => {
				return POOPTERM;
			}
		};
		Credit {
			author,
			source,
			license,
		}
	}
}

impl Credit {
	/// One line for the theme picker: "by …, MIT, source".
	pub(crate) fn line(self) -> String {
		format!("by {} · {} · {}", self.author, self.license, self.source)
	}
}
