//! What a modifier-click on a link or path does.
//!
//! Link targets come from program output, and an OSC 8 hyperlink's text can
//! differ from its target, so a link labelled `https://…` may point at a
//! local file. Opening a `.desktop` file or an executable through the
//! desktop's opener can run it, so those are copied instead of opened.
use std::path::{Path, PathBuf};

use crate::shell_integration::decode_uri_path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkAction {
	OpenUrl(String),
	OpenPath(PathBuf),
	CopyPath(String),
}

/// File extensions that desktop openers may treat as programs to launch.
const LAUNCHABLE: &[&str] = &[
	"action", "app", "appimage", "bat", "cmd", "command", "desktop", "dmg", "exe", "fileloc",
	"inetloc", "jar", "jnlp", "mpkg", "msi", "pkg", "prefpane", "ps1", "run", "scpt", "scptd", "sh",
	"terminal", "tool", "url", "webloc", "workflow",
];

/// Decides what a modifier-click on `target` (a hyperlink URI or a word
/// under the pointer) should do, or `None` if it is not a link or path.
pub(crate) fn link_action(target: &str) -> Option<LinkAction> {
	if target.starts_with("https://") || target.starts_with("http://") {
		return Some(LinkAction::OpenUrl(target.to_owned()));
	}
	if let Some(uri) = target.strip_prefix("file://") {
		let path = uri.strip_prefix("localhost").unwrap_or(uri);
		let path = path.starts_with('/').then(|| decode_uri_path(path))??;
		// Checked and opened as what it resolves to: a link's own name may
		// say nothing of its target (`readme.txt` to an app bundle).
		return Some(match path.canonicalize() {
			Ok(real) if safe_to_open(&real) => LinkAction::OpenPath(real),
			_ => LinkAction::CopyPath(path.display().to_string()),
		});
	}
	target
		.contains('/')
		.then(|| LinkAction::CopyPath(target.to_owned()))
}

/// Directories and ordinary documents; never executables or launchers.
/// `path` is resolved: no symlink stands between the check and its target.
fn safe_to_open(path: &Path) -> bool {
	let Ok(metadata) = path.metadata() else {
		return false;
	};
	if metadata.is_dir() {
		return !has_launchable_extension(path);
	}
	metadata.is_file() && !has_launchable_extension(path) && !is_executable(&metadata)
}

fn has_launchable_extension(path: &Path) -> bool {
	path
		.extension()
		.and_then(|ext| ext.to_str())
		.is_some_and(|ext| {
			LAUNCHABLE
				.iter()
				.any(|known| ext.eq_ignore_ascii_case(known))
		})
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
	use std::os::unix::fs::PermissionsExt;
	metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &std::fs::Metadata) -> bool {
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	/// A scratch directory removed when the guard drops.
	struct TempDir(PathBuf);

	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	fn temp_dir(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("poopterm-links-{}-{name}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn web_links_open_in_the_browser() {
		assert_eq!(
			link_action("https://example.com/a"),
			Some(LinkAction::OpenUrl("https://example.com/a".into()))
		);
	}

	#[test]
	fn local_documents_and_folders_open() {
		let guard = TempDir(temp_dir("docs"));
		let dir = guard.0.clone();
		let doc = dir.join("notes.txt");
		fs::write(&doc, "hi").unwrap();
		// What opens is the resolved path (macOS's /var is /private/var).
		let uri = format!("file://{}", doc.display());
		let real = |path: &PathBuf| path.canonicalize().unwrap();
		assert_eq!(link_action(&uri), Some(LinkAction::OpenPath(real(&doc))));
		let uri = format!("file://localhost{}", dir.display());
		assert_eq!(link_action(&uri), Some(LinkAction::OpenPath(real(&dir))));
	}

	#[test]
	fn launchable_files_are_copied_instead_of_opened() {
		let guard = TempDir(temp_dir("launch"));
		let dir = guard.0.clone();
		for name in ["evil.desktop", "run.command", "x.sh", "app.jar"] {
			let path = dir.join(name);
			fs::write(&path, "x").unwrap();
			let uri = format!("file://{}", path.display());
			assert_eq!(
				link_action(&uri),
				Some(LinkAction::CopyPath(path.display().to_string())),
				"{name}"
			);
		}
	}

	#[cfg(unix)]
	#[test]
	fn executable_files_are_copied_instead_of_opened() {
		use std::os::unix::fs::PermissionsExt;
		let guard = TempDir(temp_dir("exec"));
		let dir = guard.0.clone();
		let path = dir.join("tool");
		fs::write(&path, "#!/bin/sh\n").unwrap();
		fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
		let uri = format!("file://{}", path.display());
		assert_eq!(
			link_action(&uri),
			Some(LinkAction::CopyPath(path.display().to_string()))
		);
	}

	#[test]
	fn missing_files_and_plain_paths_are_copied() {
		assert_eq!(
			link_action("file:///no/such/file"),
			Some(LinkAction::CopyPath("/no/such/file".into()))
		);
		assert_eq!(
			link_action("src/main.rs"),
			Some(LinkAction::CopyPath("src/main.rs".into()))
		);
		assert_eq!(link_action("word"), None);
		assert_eq!(link_action("javascript:alert(1)"), None);
	}
}
