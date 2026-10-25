//! Font database: every installed font face, cached between runs.
//!
//! Works like fontconfig's cache: the cache records each font directory
//! with its modification time, so checking it costs one `stat` per
//! directory and reads no fonts. Adding, removing or renaming a file changes
//! its directory's time; a stale cache rescans the directories and reads
//! only fonts whose time or size changed. Fonts rewritten in place without
//! touching their directory are picked up on the next directory change.
use std::{
	collections::HashMap,
	fs,
	io::Read,
	path::{Path, PathBuf},
	thread,
	time::UNIX_EPOCH,
};

use super::{
	coverage,
	sfnt::{FaceInfo, read_faces},
};

/// Bounds on what a scan visits; real systems stay far below them.
const MAX_DEPTH: usize = 8;
const MAX_DIRS: usize = 20_000;
const MAX_FILES: usize = 200_000;
/// A cache of 200,000 faces is about 40 MB.
const MAX_CACHE_FILE: u64 = 64 << 20;
/// Paths with their stamps.
type Stamped = Vec<(PathBuf, Stamp)>;
/// Bumped when face data changes meaning, so old caches are rebuilt.
const HEADER: &str = "poopterm-fonts 3";

/// Modification time in nanoseconds and size. A missing path has the
/// default stamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
	pub mtime: i128,
	pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontFile {
	pub path: PathBuf,
	pub stamp: Stamp,
	/// Empty for files that are not fonts, so they are not read again.
	pub faces: Vec<FaceInfo>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cache {
	pub roots: Vec<PathBuf>,
	pub dirs: Vec<(PathBuf, Stamp)>,
	pub files: Vec<FontFile>,
}

impl Cache {
	/// Every face with its file and index in that file.
	pub(crate) fn faces(&self) -> impl Iterator<Item = ((&Path, u32), &FaceInfo)> {
		self.files.iter().flat_map(|file| {
			file
				.faces
				.iter()
				.map(move |face| ((file.path.as_path(), face.index), face))
		})
	}

	/// The face of `family` (any case) closest to `weight` and `italic`:
	/// the slant decides first, then the nearest weight, ties going heavier
	/// for bold requests and lighter otherwise, as CSS does.
	pub(crate) fn find(&self, family: &str, weight: u16, italic: bool) -> Option<(&Path, u32)> {
		self
			.faces()
			.filter(|(_, face)| face.family.eq_ignore_ascii_case(family))
			.min_by_key(|(_, face)| {
				let wrong_side = if weight > 500 {
					face.weight < weight
				} else {
					face.weight > weight
				};
				(
					face.italic != italic,
					face.weight.abs_diff(weight),
					wrong_side,
				)
			})
			.map(|(found, _)| found)
	}

	/// Monospace family names, sorted and deduplicated ignoring case.
	pub(crate) fn monospace_families(&self) -> Vec<String> {
		let mut names: Vec<&str> = self
			.faces()
			.filter(|(_, face)| face.monospace)
			.map(|(_, face)| face.family.as_str())
			.collect();
		names.sort_by_cached_key(|name| (name.to_lowercase(), *name));
		names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
		names.into_iter().map(str::to_owned).collect()
	}
}

/// The installed fonts, from the cache file when it is fresh. A stale or
/// missing cache is rebuilt and saved.
pub(crate) fn load(roots: &[PathBuf], cache_path: &Path) -> Cache {
	let cached = fs::File::open(cache_path).ok().and_then(|file| {
		let mut text = String::new();
		file.take(MAX_CACHE_FILE).read_to_string(&mut text).ok()?;
		decode(&text)
	});
	if let Some(cache) = cached.as_ref().filter(|cache| is_fresh(cache, roots)) {
		return cache.clone();
	}
	let (cache, _) = refresh(roots, cached);
	if let Some(parent) = cache_path.parent() {
		let temporary = cache_path.with_extension(format!("{}.tmp", std::process::id()));
		let saved = fs::create_dir_all(parent)
			.and_then(|()| fs::write(&temporary, encode(&cache)))
			.and_then(|()| fs::rename(&temporary, cache_path));
		if saved.is_err() {
			let _ = fs::remove_file(&temporary);
		}
	}
	cache
}

/// Whether `cache` was built from `roots` and no directory changed since.
pub(crate) fn is_fresh(cache: &Cache, roots: &[PathBuf]) -> bool {
	cache.roots == roots
		&& cache
			.dirs
			.iter()
			.all(|(dir, stamp)| self::stamp(dir) == *stamp)
}

/// Brings `cached` up to date with the fonts under `roots`, returning the
/// new cache and how many font files had to be read.
pub(crate) fn refresh(roots: &[PathBuf], cached: Option<Cache>) -> (Cache, usize) {
	if let Some(cache) = cached.as_ref().filter(|cache| is_fresh(cache, roots)) {
		return (cache.clone(), 0);
	}
	let (dirs, found) = walk(roots);
	let mut old: HashMap<PathBuf, FontFile> = cached
		.map(|cache| {
			cache
				.files
				.into_iter()
				.map(|file| (file.path.clone(), file))
				.collect()
		})
		.unwrap_or_default();
	let mut files = Vec::with_capacity(found.len());
	let mut unread = Vec::new();
	for (path, stamp) in found {
		match old.remove(&path) {
			Some(file) if file.stamp == stamp => files.push(file),
			_ => {
				unread.push(files.len());
				files.push(FontFile {
					path,
					stamp,
					faces: Vec::new(),
				});
			}
		}
	}
	read_in_parallel(&mut files, &unread);
	let cache = Cache {
		roots: roots.to_vec(),
		dirs,
		files,
	};
	(cache, unread.len())
}

/// Reads the faces of `files[i]` for each `i` in `unread`. Cold reads wait
/// on the disk, so several threads keep more requests in flight.
fn read_in_parallel(files: &mut [FontFile], unread: &[usize]) {
	let threads = thread::available_parallelism()
		.map_or(4, |n| n.get())
		.clamp(1, 8);
	let read = |path: &Path| {
		fs::File::open(path)
			.and_then(|mut file| read_faces(&mut file))
			.unwrap_or_default()
	};
	let paths: Vec<&Path> = unread.iter().map(|&i| files[i].path.as_path()).collect();
	let faces: Vec<Vec<FaceInfo>> = if paths.len() < 16 {
		paths.iter().map(|path| read(path)).collect()
	} else {
		let per_thread = paths.len().div_ceil(threads);
		thread::scope(|scope| {
			let workers: Vec<_> = paths
				.chunks(per_thread)
				.map(|chunk| scope.spawn(move || chunk.iter().map(|path| read(path)).collect::<Vec<_>>()))
				.collect();
			workers
				.into_iter()
				.flat_map(|worker| worker.join().unwrap_or_default())
				.collect()
		})
	};
	for (&i, faces) in unread.iter().zip(faces) {
		files[i].faces = faces;
	}
}

fn stamp(path: &Path) -> Stamp {
	fs::metadata(path).map_or_else(|_| Stamp::default(), |meta| stamp_of(&meta))
}

fn stamp_of(meta: &fs::Metadata) -> Stamp {
	let mtime = meta
		.modified()
		.ok()
		.map_or(0, |time| match time.duration_since(UNIX_EPOCH) {
			Ok(after) => after.as_nanos() as i128,
			Err(before) => -(before.duration().as_nanos() as i128),
		});
	Stamp {
		mtime,
		size: if meta.is_dir() { 0 } else { meta.len() },
	}
}

fn is_font(path: &Path) -> bool {
	path
		.extension()
		.and_then(|ext| ext.to_str())
		.is_some_and(|ext| {
			["ttf", "otf", "ttc", "otc"]
				.iter()
				.any(|font| ext.eq_ignore_ascii_case(font))
		})
}

/// Every directory under `roots` (missing roots too, so their creation is
/// noticed) and every font file, in a stable order. Paths that are not
/// UTF-8 are skipped, since the cache file is text.
fn walk(roots: &[PathBuf]) -> (Stamped, Stamped) {
	let (mut dirs, mut files) = (Vec::new(), Vec::new());
	let mut stack: Vec<(PathBuf, usize)> = roots.iter().rev().map(|root| (root.clone(), 0)).collect();
	while let Some((dir, depth)) = stack.pop() {
		if dirs.len() >= MAX_DIRS {
			break;
		}
		dirs.push((dir.clone(), stamp(&dir)));
		let Ok(entries) = fs::read_dir(&dir) else {
			continue;
		};
		let mut entries: Vec<PathBuf> = entries
			.flatten()
			.map(|entry| entry.path())
			.filter(|path| path.to_str().is_some())
			.collect();
		entries.sort_unstable();
		let mut subdirs = Vec::new();
		for path in entries {
			// Follows links, as font directories often link to others.
			let Ok(meta) = fs::metadata(&path) else {
				continue;
			};
			if meta.is_dir() {
				if depth < MAX_DEPTH {
					subdirs.push((path, depth + 1));
				}
			} else if meta.is_file() && is_font(&path) && files.len() < MAX_FILES {
				files.push((path, stamp_of(&meta)));
			}
		}
		stack.extend(subdirs.into_iter().rev());
	}
	(dirs, files)
}

/// The cache as text: a header, then one tab-separated record per line, and
/// `end` so a truncated file is rejected.
pub(crate) fn encode(cache: &Cache) -> String {
	let mut out = format!("{HEADER}\n");
	for root in &cache.roots {
		out += &format!("R\t{}\n", escape(&root.to_string_lossy()));
	}
	for (dir, stamp) in &cache.dirs {
		out += &format!("D\t{}\t{}\n", stamp.mtime, escape(&dir.to_string_lossy()));
	}
	for file in &cache.files {
		let FontFile { path, stamp, faces } = file;
		out += &format!(
			"F\t{}\t{}\t{}\n",
			stamp.mtime,
			stamp.size,
			escape(&path.to_string_lossy())
		);
		for face in faces {
			out += &format!(
				"f\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
				face.index,
				face.weight,
				u8::from(face.italic),
				u8::from(face.monospace),
				u8::from(face.outlines),
				coverage::encode(&face.pages),
				escape(&face.family),
				escape(&face.style),
				escape(&face.full_name),
				escape(&face.postscript),
			);
		}
	}
	out + "end\n"
}

/// Parses [`encode`]'s output; `None` for anything else.
pub(crate) fn decode(text: &str) -> Option<Cache> {
	let mut lines = text.lines();
	if lines.next()? != HEADER {
		return None;
	}
	let mut cache = Cache::default();
	for line in lines {
		let fields: Vec<&str> = line.split('\t').collect();
		match fields.as_slice() {
			["R", path] => cache.roots.push(unescape(path)?.into()),
			["D", mtime, path] => cache.dirs.push((
				unescape(path)?.into(),
				Stamp {
					mtime: mtime.parse().ok()?,
					size: 0,
				},
			)),
			["F", mtime, size, path] => cache.files.push(FontFile {
				path: unescape(path)?.into(),
				stamp: Stamp {
					mtime: mtime.parse().ok()?,
					size: size.parse().ok()?,
				},
				faces: Vec::new(),
			}),
			[
				"f",
				index,
				weight,
				italic,
				monospace,
				outlines,
				pages,
				family,
				style,
				full_name,
				postscript,
			] => {
				cache.files.last_mut()?.faces.push(FaceInfo {
					index: index.parse().ok()?,
					weight: weight.parse().ok()?,
					italic: *italic == "1",
					monospace: *monospace == "1",
					outlines: *outlines == "1",
					pages: coverage::decode(pages)?,
					family: unescape(family)?,
					style: unescape(style)?,
					full_name: unescape(full_name)?,
					postscript: unescape(postscript)?,
				});
			}
			["end"] => return Some(cache),
			_ => return None,
		}
	}
	None
}

fn escape(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for ch in text.chars() {
		match ch {
			'\\' => out += "\\\\",
			'\t' => out += "\\t",
			'\n' => out += "\\n",
			'\r' => out += "\\r",
			ch => out.push(ch),
		}
	}
	out
}

fn unescape(text: &str) -> Option<String> {
	let mut out = String::with_capacity(text.len());
	let mut chars = text.chars();
	while let Some(ch) = chars.next() {
		out.push(match ch {
			'\\' => match chars.next()? {
				'\\' => '\\',
				't' => '\t',
				'n' => '\n',
				'r' => '\r',
				_ => return None,
			},
			ch => ch,
		});
	}
	Some(out)
}
