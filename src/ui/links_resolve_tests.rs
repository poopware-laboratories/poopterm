//! What a modifier-click does with links whose names say less than their
//! targets: symlinks, chains of them, `..` and launchers of every platform.
//! The checks apply to what a path resolves to, and that is what opens.
use super::links::{LinkAction, link_action};
use std::{fs, os::unix::fs::symlink, path::PathBuf};

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn scratch(name: &str) -> Scratch {
	let dir = std::env::temp_dir().join(format!("poopterm-resolve-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	// Resolved, so expected paths match what the checks compare.
	Scratch(dir.canonicalize().unwrap())
}

fn click(path: &std::path::Path) -> Option<LinkAction> {
	link_action(&format!("file://{}", path.display()))
}

fn copied(path: &std::path::Path) -> Option<LinkAction> {
	Some(LinkAction::CopyPath(path.display().to_string()))
}

#[test]
fn a_symlink_named_like_a_document_to_an_app_bundle_is_copied() {
	let dir = scratch("bundle");
	let app = dir.0.join("Calculator.app");
	fs::create_dir(&app).unwrap();
	let link = dir.0.join("readme.txt");
	symlink(&app, &link).unwrap();
	assert_eq!(click(&link), copied(&link));
	assert_eq!(click(&app), copied(&app), "the bundle itself, as before");
}

#[test]
fn a_symlink_to_a_launcher_that_is_not_executable_is_copied() {
	let dir = scratch("launcher");
	for target in ["evil.desktop", "run.command", "open.fileloc", "go.webloc"] {
		let file = dir.0.join(target);
		fs::write(&file, "x").unwrap();
		let link = dir.0.join(format!("{target}.txt"));
		symlink(&file, &link).unwrap();
		assert_eq!(click(&link), copied(&link), "{target}");
	}
}

#[test]
fn chains_of_symlinks_and_dotdot_resolve_before_the_check() {
	let dir = scratch("chain");
	let app = dir.0.join("Tool.app");
	fs::create_dir(&app).unwrap();
	let (first, second) = (dir.0.join("a.txt"), dir.0.join("b.md"));
	symlink(&app, &second).unwrap();
	symlink(&second, &first).unwrap();
	assert_eq!(click(&first), copied(&first), "two links deep");
	let sub = dir.0.join("sub");
	fs::create_dir(&sub).unwrap();
	let dotted = sub.join("../Tool.app");
	assert_eq!(click(&dotted), copied(&dotted), "through ..");
}

#[test]
fn a_symlink_to_a_document_opens_the_document_itself() {
	let dir = scratch("document");
	let notes = dir.0.join("notes.md");
	fs::write(&notes, "hi").unwrap();
	let link = dir.0.join("latest");
	symlink(&notes, &link).unwrap();
	// The opener gets the resolved path, so it cannot be swapped for
	// another target between the check and the open.
	assert_eq!(click(&link), Some(LinkAction::OpenPath(notes.clone())));
	let folder = dir.0.join("folder");
	fs::create_dir(&folder).unwrap();
	let folder_link = dir.0.join("shortcut");
	symlink(&folder, &folder_link).unwrap();
	assert_eq!(click(&folder_link), Some(LinkAction::OpenPath(folder)));
}

#[test]
fn dangling_and_looping_symlinks_are_copied() {
	let dir = scratch("dangling");
	let dangling = dir.0.join("gone.txt");
	symlink(dir.0.join("nothing-here"), &dangling).unwrap();
	assert_eq!(click(&dangling), copied(&dangling));
	let (a, b) = (dir.0.join("a.txt"), dir.0.join("b.txt"));
	symlink(&b, &a).unwrap();
	symlink(&a, &b).unwrap();
	assert_eq!(click(&a), copied(&a), "a loop");
}

#[test]
fn launchers_of_every_platform_are_copied_whatever_their_case() {
	let dir = scratch("kinds");
	let kinds = [
		"open.fileloc",
		"run.action",
		"pane.prefPane",
		"script.scptd",
		"site.webloc",
		"site.inetloc",
		"setup.mpkg",
		"start.jnlp",
		"disk.dmg",
		"link.url",
		"SHOUT.DESKTOP",
	];
	for name in kinds {
		let path = dir.0.join(name);
		fs::write(&path, "x").unwrap();
		assert_eq!(click(&path), copied(&path), "{name}");
	}
	// Bundles of those kinds are directories on macOS.
	let pane = dir.0.join("Other.prefPane");
	fs::create_dir(&pane).unwrap();
	assert_eq!(click(&pane), copied(&pane));
}
