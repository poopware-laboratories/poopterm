//! Terminal font loading with bundled and system symbol-font fallbacks.
mod cbdt;
#[cfg(test)]
pub(crate) mod cbdt_tests;
mod cff;
mod cmap;
mod color;
pub(crate) mod coverage;
pub(crate) mod db;
#[cfg(test)]
mod db_tests;
mod face;
pub(crate) mod fallback;
#[cfg(test)]
pub(crate) mod fallback_tests;
mod font;
#[cfg(test)]
mod font_bench;
#[cfg(test)]
mod font_tests;
mod glyf;
mod gsub;
#[cfg(test)]
pub(crate) use gsub::tests as gsub_tests;
mod gsub_context;
mod otl;
mod raster;
mod read;
mod sfnt;
#[cfg(test)]
mod sfnt_tests;

use std::{
	env,
	ffi::OsString,
	fs,
	path::{Path, PathBuf},
};

pub(crate) use font::{Font, Metrics};

use crate::{
	config::{cache_dir, read_config_value, xdg_base},
	session_store::default_cwd,
};

/// The font that ships inside the binary.
pub(crate) const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

pub(crate) fn load_fonts() -> (Font, Vec<Font>) {
	let bundled = Font::from_bytes(BUNDLED, 0).expect("bundled font is valid");
	let source = font_source(
		env::var("POOPTERM_FONT").ok(),
		read_config_value("font_path"),
		read_config_value("font_family"),
	);
	let (font, mut fallback_fonts) = match source.and_then(locate) {
		Some((path, index)) => match load_font(&path, index) {
			Ok(font) => (font, vec![bundled]),
			Err(error) => {
				eprintln!(
					"poopterm: couldn't load font {}: {error}; using bundled font",
					path.display()
				);
				(bundled, Vec::new())
			}
		},
		None => (bundled, Vec::new()),
	};

	for path in installed_symbol_font_paths().into_iter().take(8) {
		let Ok(bytes) = fs::read(&path) else {
			continue;
		};
		if bytes.len() > 64 * 1024 * 1024 {
			continue;
		}
		if let Ok(font) = Font::from_bytes(bytes, 0) {
			fallback_fonts.push(font);
		}
	}
	(font, fallback_fonts)
}

/// Where the terminal font comes from, by precedence: `POOPTERM_FONT`,
/// `font_path`, then `font_family` looked up in the font database.
#[derive(Debug, PartialEq, Eq)]
enum FontSource {
	File(PathBuf),
	Family(String),
}

fn font_source(
	env_font: Option<String>,
	font_path: Option<String>,
	font_family: Option<String>,
) -> Option<FontSource> {
	let set = |value: Option<String>| {
		value
			.map(|text| text.trim().to_owned())
			.filter(|text| !text.is_empty())
	};
	set(env_font)
		.or_else(|| set(font_path))
		.map(|path| {
			FontSource::File(
				path
					.strip_prefix("~/")
					.map_or_else(|| PathBuf::from(&path), |tail| default_cwd().join(tail)),
			)
		})
		.or_else(|| set(font_family).map(FontSource::Family))
}

/// The file and collection index of a font source; a family missing from
/// the database is reported and falls back to the bundled font.
fn locate(source: FontSource) -> Option<(PathBuf, u32)> {
	match source {
		FontSource::File(path) => Some((path, 0)),
		FontSource::Family(family) => {
			let found = system_fonts()
				.find(&family, 400, false)
				.map(|(path, index)| (path.to_owned(), index));
			if found.is_none() {
				eprintln!("poopterm: no installed font family {family:?}; using bundled font");
			}
			found
		}
	}
}

fn load_font(path: &Path, collection_index: u32) -> Result<Font, String> {
	let bytes = fs::read(path).map_err(|error| error.to_string())?;
	Font::from_bytes(bytes, collection_index).map_err(|error| error.to_string())
}

/// The installed fonts, from the cache in `$XDG_CACHE_HOME/poopterm`.
pub(crate) fn system_fonts() -> db::Cache {
	let roots = font_roots(
		env::var_os("HOME"),
		env::var_os("XDG_DATA_HOME"),
		env::var_os("XDG_DATA_DIRS"),
	);
	match cache_dir() {
		Some(dir) => db::load(&roots, &dir.join("fonts.db")),
		None => db::refresh(&roots, None).0,
	}
}

pub(crate) fn installed_symbol_font_paths() -> Vec<PathBuf> {
	pub(crate) fn scan(path: &PathBuf, depth: usize, budget: &mut usize, out: &mut Vec<PathBuf>) {
		if depth > 5 || *budget == 0 {
			return;
		}
		let Ok(entries) = fs::read_dir(path) else {
			return;
		};
		for entry in entries.flatten() {
			if *budget == 0 {
				break;
			}
			*budget -= 1;
			let path = entry.path();
			if path.is_dir() {
				scan(&path, depth + 1, budget, out);
				continue;
			}
			let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
				continue;
			};
			let filename = filename.to_ascii_lowercase();
			let symbol_font =
				filename.contains("nerd") || filename.contains("symbol") || filename.contains("awesome");
			if symbol_font
				&& path
					.extension()
					.is_some_and(|ext| ext.eq_ignore_ascii_case("ttf"))
			{
				out.push(path);
			}
		}
	}

	let roots = font_roots(
		env::var_os("HOME"),
		env::var_os("XDG_DATA_HOME"),
		env::var_os("XDG_DATA_DIRS"),
	);
	let mut out = Vec::new();
	let mut budget = 2048;
	for root in roots {
		scan(&root, 0, &mut budget, &mut out);
		if budget == 0 {
			break;
		}
	}
	out.sort();
	out.dedup();
	out
}

/// Font directories: `fonts` in each XDG data directory, then the macOS and
/// legacy (`~/.fonts`) locations.
fn font_roots(
	home: Option<OsString>,
	data_home: Option<OsString>,
	data_dirs: Option<OsString>,
) -> Vec<PathBuf> {
	let home = home.map(PathBuf::from);
	let data_dirs = data_dirs.filter(|dirs| !dirs.is_empty());
	let mut roots: Vec<PathBuf> = env::split_paths(
		data_dirs
			.as_deref()
			.unwrap_or("/usr/local/share:/usr/share".as_ref()),
	)
	.chain(xdg_base(data_home, home.as_deref(), ".local/share"))
	.filter(|dir| dir.is_absolute())
	.map(|dir| dir.join("fonts"))
	.collect();
	roots.extend(["/Library/Fonts", "/System/Library/Fonts"].map(PathBuf::from));
	if let Some(home) = home {
		roots.extend([home.join(".fonts"), home.join("Library/Fonts")]);
	}
	let mut seen = std::collections::HashSet::new();
	roots.retain(|dir| seen.insert(dir.clone()));
	roots
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn font_file_settings_win_over_the_family() {
		let some = |text: &str| Some(text.to_owned());
		let source = |env, path, family| font_source(env, path, family);
		assert_eq!(
			source(some("/env.ttf"), some("/path.ttf"), some("Iosevka")),
			Some(FontSource::File("/env.ttf".into()))
		);
		assert_eq!(
			source(None, some("/path.ttf"), some("Iosevka")),
			Some(FontSource::File("/path.ttf".into()))
		);
		assert_eq!(
			source(None, None, some("  Iosevka  ")),
			Some(FontSource::Family("Iosevka".into()))
		);
		assert_eq!(source(None, some(""), some(" ")), None);
		assert_eq!(source(None, None, None), None);
	}

	#[test]
	fn font_directories_follow_xdg_data_variables() {
		let roots = font_roots(
			Some("/h".into()),
			Some("/data".into()),
			Some("/a:relative::/usr/share".into()),
		);
		for dir in ["/a/fonts", "/usr/share/fonts", "/data/fonts", "/h/.fonts"] {
			assert!(roots.contains(&PathBuf::from(dir)), "{dir} in {roots:?}");
		}
		assert!(!roots.iter().any(|dir| dir.is_relative()), "{roots:?}");
		assert_eq!(
			roots.len(),
			roots
				.iter()
				.collect::<std::collections::BTreeSet<_>>()
				.len()
		);
		let defaults = font_roots(Some("/h".into()), None, None);
		for dir in [
			"/usr/share/fonts",
			"/usr/local/share/fonts",
			"/h/.local/share/fonts",
		] {
			assert!(
				defaults.contains(&PathBuf::from(dir)),
				"{dir} in {defaults:?}"
			);
		}
	}
}
