//! Tests for the font database: scanning, cache reuse and invalidation,
//! the cache file format, and face matching.
use super::db::*;
use super::sfnt::FaceInfo;
use std::{
	fs,
	path::{Path, PathBuf},
	time::{Duration, SystemTime},
};

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

/// An empty directory for one test.
fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-fontdb-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir
}

/// Sets a directory's mtime, as adding a file a few seconds later would.
/// File timestamps come from a coarse clock, so tests cannot rely on time
/// passing between two writes.
fn touch(dir: &Path, seconds: u64) {
	let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + seconds);
	fs::File::open(dir).unwrap().set_modified(time).unwrap();
}

#[test]
fn scans_font_files_in_nested_directories() {
	let root = scratch("nested");
	fs::create_dir_all(root.join("a/b")).unwrap();
	fs::write(root.join("a/b/Mono.TTF"), BUNDLED).unwrap();
	fs::write(root.join("a/readme.txt"), "not a font").unwrap();
	fs::write(root.join("broken.otf"), "not a font either").unwrap();
	let (cache, parsed) = refresh(&[root.clone(), root.join("missing")], None);
	assert_eq!(parsed, 2, "both font-named files are read");
	let faces: Vec<_> = cache.faces().collect();
	assert_eq!(faces.len(), 1);
	assert_eq!(faces[0].0, (root.join("a/b/Mono.TTF").as_path(), 0));
	assert_eq!(faces[0].1.family, "DejaVu Sans Mono for Powerline");
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn unchanged_directories_reuse_the_cache_without_reading_fonts() {
	let root = scratch("warm");
	fs::write(root.join("one.ttf"), BUNDLED).unwrap();
	touch(&root, 1);
	let (cache, _) = refresh(std::slice::from_ref(&root), None);
	let (again, parsed) = refresh(std::slice::from_ref(&root), Some(cache.clone()));
	assert_eq!(parsed, 0);
	assert_eq!(again, cache);
	assert!(is_fresh(&cache, std::slice::from_ref(&root)));
	// Other roots make the cache stale even if every directory matches.
	assert!(!is_fresh(&cache, &[root.clone(), root.join("new")]));
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn changed_directories_read_only_new_or_changed_files() {
	let root = scratch("changed");
	fs::create_dir_all(root.join("sub")).unwrap();
	fs::write(root.join("sub/one.ttf"), BUNDLED).unwrap();
	touch(&root.join("sub"), 1);
	let (cache, _) = refresh(std::slice::from_ref(&root), None);

	fs::write(root.join("sub/two.ttf"), BUNDLED).unwrap();
	touch(&root.join("sub"), 2);
	assert!(!is_fresh(&cache, std::slice::from_ref(&root)));
	let (cache, parsed) = refresh(std::slice::from_ref(&root), Some(cache));
	assert_eq!(parsed, 1, "only the new file is read");
	assert_eq!(cache.faces().count(), 2);

	fs::remove_file(root.join("sub/one.ttf")).unwrap();
	touch(&root.join("sub"), 3);
	let (cache, parsed) = refresh(std::slice::from_ref(&root), Some(cache));
	assert_eq!(parsed, 0);
	let paths: Vec<_> = cache
		.faces()
		.map(|((path, _), _)| path.to_owned())
		.collect();
	assert_eq!(paths, [root.join("sub/two.ttf")]);
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_cache_file_round_trips_and_rejects_garbage() {
	let root = scratch("roundtrip");
	fs::write(root.join("one.ttf"), BUNDLED).unwrap();
	let (mut cache, _) = refresh(std::slice::from_ref(&root), None);
	// Names come from untrusted fonts: separators must not break the format.
	cache.files[0].faces[0].family = "Evil\tName\nWith\\Escapes".into();
	let text = encode(&cache);
	assert_eq!(decode(&text), Some(cache));
	assert_eq!(decode(""), None);
	assert_eq!(decode("poopterm-fonts 0\n"), None);
	assert_eq!(
		decode(&text.replace("poopterm-fonts", "something-else")),
		None
	);
	// Truncated or corrupted caches are rejected or partially read, never a panic.
	for len in 0..text.len() {
		if text.is_char_boundary(len) {
			let _ = decode(&text[..len]);
		}
	}
	fs::remove_dir_all(root).unwrap();
}

fn face(family: &str, weight: u16, italic: bool, monospace: bool) -> FaceInfo {
	FaceInfo {
		index: 0,
		family: family.into(),
		style: String::new(),
		full_name: String::new(),
		postscript: String::new(),
		weight,
		italic,
		monospace,
		outlines: true,
		pages: vec![(0, 0)],
	}
}

fn synthetic(faces: &[FaceInfo]) -> Cache {
	Cache {
		roots: Vec::new(),
		dirs: Vec::new(),
		files: faces
			.iter()
			.enumerate()
			.map(|(i, face)| FontFile {
				path: format!("/fonts/{i}.ttf").into(),
				stamp: Stamp::default(),
				faces: vec![face.clone()],
			})
			.collect(),
	}
}

#[test]
fn matching_prefers_the_slant_then_the_nearest_weight() {
	let cache = synthetic(&[
		face("Iosevka", 400, false, true),
		face("Iosevka", 700, false, true),
		face("Iosevka", 400, true, true),
		face("Other", 400, false, true),
	]);
	let path = |found: Option<(&Path, u32)>| found.map(|(path, _)| path.to_owned());
	assert_eq!(
		path(cache.find("iosevka", 400, false)),
		Some("/fonts/0.ttf".into())
	);
	assert_eq!(
		path(cache.find("Iosevka", 800, false)),
		Some("/fonts/1.ttf".into())
	);
	assert_eq!(
		path(cache.find("Iosevka", 700, true)),
		Some("/fonts/2.ttf".into())
	);
	assert_eq!(path(cache.find("Missing", 400, false)), None);
}

#[test]
fn monospace_families_are_listed_once_in_order() {
	let cache = synthetic(&[
		face("Zeta Mono", 400, false, true),
		face("alpha mono", 700, false, true),
		face("Zeta Mono", 700, false, true),
		face("Sans", 400, false, false),
	]);
	assert_eq!(cache.monospace_families(), ["alpha mono", "Zeta Mono"]);
	assert_eq!(cache.faces().count(), 4);
}

#[test]
fn the_cache_keeps_each_faces_pages_and_outline_flag() {
	let root = scratch("pages");
	fs::write(root.join("one.ttf"), BUNDLED).unwrap();
	let (mut cache, _) = refresh(std::slice::from_ref(&root), None);
	let face = &cache.files[0].faces[0];
	assert!(face.outlines && face.pages.len() > 1, "{face:?}");
	let text = encode(&cache);
	assert_eq!(decode(&text), Some(cache.clone()));
	cache.files[0].faces[0].outlines = false;
	cache.files[0].faces[0].pages = vec![(0x1f3, 0x1fa)];
	let text = encode(&cache);
	assert!(text.contains("\t0\t1f3-1fa\t"), "{text}");
	assert_eq!(decode(&text), Some(cache));
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn caches_written_before_page_coverage_are_rebuilt() {
	// A version 2 face line has no outline flag or pages.
	let old = "poopterm-fonts 2\nF\t1\t2\t/f.ttf\nf\t0\t400\t0\t1\tMono\tRegular\tMono\tMono\nend\n";
	assert_eq!(decode(old), None);
	let current = old.replace("fonts 2", "fonts 3");
	assert_eq!(
		decode(&current),
		None,
		"the old face line is not read either"
	);
	let fixed = current.replace("\t1\tMono\t", "\t1\t1\t0-2\tMono\t");
	let cache = decode(&fixed).unwrap();
	assert_eq!(cache.files[0].faces[0].pages, [(0, 2)]);
	assert_eq!(decode(&fixed.replace("0-2", "2-0")), None, "bad pages");
}
