//! Saved tab working directories (`sessions.txt` in the state directory).
use std::{
	env, fs,
	path::{Path, PathBuf},
};

use crate::config::{Preferences, config_path, state_dir};

/// Where tabs are saved; falls back beside the config file without `HOME`.
fn session_path() -> PathBuf {
	state_dir().map_or_else(legacy_session_path, |dir| dir.join("sessions.txt"))
}

/// Where versions before the XDG state directory saved tabs.
fn legacy_session_path() -> PathBuf {
	config_path().with_file_name("sessions.txt")
}

// Session state is separate from user settings; hex encoding preserves unusual paths.
pub(crate) fn read_session_dirs() -> Vec<PathBuf> {
	read_session_dirs_from(&[&session_path(), &legacy_session_path()])
}

/// Reads the first of `paths` that exists.
fn read_session_dirs_from(paths: &[&Path]) -> Vec<PathBuf> {
	paths
		.iter()
		.find_map(|path| fs::read_to_string(path).ok())
		.unwrap_or_default()
		.lines()
		.filter_map(decode_session_path)
		.take(12)
		.collect()
}

pub(crate) fn decode_session_path(line: &str) -> Option<PathBuf> {
	if !line.len().is_multiple_of(2) {
		return None;
	}
	let bytes = line
		.as_bytes()
		.as_chunks::<2>()
		.0
		.iter()
		.map(|pair| {
			let text = std::str::from_utf8(pair).ok()?;
			u8::from_str_radix(text, 16).ok()
		})
		.collect::<Option<Vec<_>>>()?;
	String::from_utf8(bytes).ok().map(PathBuf::from)
}

pub(crate) fn save_session_dirs(dirs: &[PathBuf]) -> std::io::Result<()> {
	let path = session_path();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let text = dirs
		.iter()
		.take(12)
		.map(|dir| {
			dir
				.to_string_lossy()
				.as_bytes()
				.iter()
				.map(|b| format!("{b:02x}"))
				.collect::<String>()
		})
		.collect::<Vec<_>>()
		.join("\n");
	let temp = path.with_extension(format!("{}.tmp", std::process::id()));
	fs::write(&temp, text)?;
	fs::rename(temp, path)
}

pub(crate) fn default_cwd() -> PathBuf {
	crate::config::home_dir()
		.or_else(|| env::current_dir().ok())
		.unwrap_or_else(|| PathBuf::from("/"))
}

pub(crate) fn restored_session_dirs() -> Vec<PathBuf> {
	if !Preferences::load().restore_sessions {
		return vec![default_cwd()];
	}
	let dirs: Vec<_> = read_session_dirs()
		.into_iter()
		.map(|dir| if dir.is_dir() { dir } else { default_cwd() })
		.collect();
	if dirs.is_empty() {
		vec![default_cwd()]
	} else {
		dirs
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn session_paths_preserve_punctuation() {
		let path = "/tmp/a,b#c \"quoted\"/猫";
		let encoded = path
			.as_bytes()
			.iter()
			.map(|b| format!("{b:02x}"))
			.collect::<String>();
		assert_eq!(decode_session_path(&encoded), Some(path.into()));
		assert!(decode_session_path("xyz").is_none());
	}

	#[test]
	fn sessions_saved_beside_the_config_are_still_restored() {
		let dir = env::temp_dir().join(format!("poopterm-sessions-{}", std::process::id()));
		fs::create_dir_all(&dir).unwrap();
		let (state, legacy) = (dir.join("state.txt"), dir.join("legacy.txt"));
		fs::write(&legacy, "2f746d70").unwrap();
		assert_eq!(
			read_session_dirs_from(&[&state, &legacy]),
			[PathBuf::from("/tmp")]
		);
		fs::write(&state, "2f").unwrap();
		assert_eq!(
			read_session_dirs_from(&[&state, &legacy]),
			[PathBuf::from("/")]
		);
		fs::remove_dir_all(dir).unwrap();
	}
}
