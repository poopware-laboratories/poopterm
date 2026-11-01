//! The installed symbol fonts loaded at startup, chosen from the font
//! database by rank and coverage, never by directory order.
use std::{cmp::Reverse, path::PathBuf};

use super::{db::Cache, fallback::MAX_FILE};

/// Up to `limit` symbol fonts (file and face index), best first: each adds
/// characters the ones before it lack. Faces rank by how many characters
/// they map, then upright before italic, the weight nearest regular, the
/// smaller file and the path, so no tie depends on the database's order.
pub(crate) fn symbol_fonts(cache: &Cache, limit: usize) -> Vec<(PathBuf, u32)> {
	let mut faces: Vec<_> = cache
		.files
		.iter()
		.filter(|file| file.stamp.size <= MAX_FILE)
		.flat_map(|file| file.faces.iter().map(move |face| (file, face)))
		.filter(|(file, face)| {
			let name = file.path.file_name().unwrap_or_default().to_string_lossy();
			face.drawable && !face.coverage.is_empty() && (is_symbol(&name) || is_symbol(&face.family))
		})
		.collect();
	faces.sort_by_cached_key(|(file, face)| {
		let off_regular = face.weight.abs_diff(400);
		(
			Reverse(count(&face.coverage)),
			face.italic,
			off_regular,
			file.stamp.size,
			&file.path,
			face.index,
		)
	});
	let mut covered = Vec::new();
	let mut chosen = Vec::new();
	for (file, face) in faces {
		if chosen.len() == limit {
			break;
		}
		if adds(&covered, &face.coverage) {
			covered = union(&covered, &face.coverage);
			chosen.push((file.path.clone(), face.index));
		}
	}
	chosen
}

fn is_symbol(name: &str) -> bool {
	let name = name.to_ascii_lowercase();
	["nerd", "symbol", "awesome"]
		.iter()
		.any(|word| name.contains(word))
}

/// Code points in sorted, inclusive ranges.
pub(crate) fn count(ranges: &[(u32, u32)]) -> u64 {
	ranges
		.iter()
		.map(|&(first, last)| u64::from(last - first) + 1)
		.sum()
}

/// Whether some range of `ranges` holds a code point `covered` lacks;
/// `covered` is sorted and merged, so one range must hold each of them.
pub(crate) fn adds(covered: &[(u32, u32)], ranges: &[(u32, u32)]) -> bool {
	ranges.iter().any(|&(first, last)| {
		let at = covered.partition_point(|&(_, end)| end < first);
		covered
			.get(at)
			.is_none_or(|&(start, end)| start > first || end < last)
	})
}

/// Both sets of ranges as one, sorted, with touching ranges merged.
pub(crate) fn union(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
	let mut all: Vec<_> = a.iter().chain(b).copied().collect();
	all.sort_unstable();
	let mut merged: Vec<(u32, u32)> = Vec::with_capacity(all.len());
	for (first, last) in all {
		match merged.last_mut() {
			Some((_, end)) if first <= end.saturating_add(1) => *end = (*end).max(last),
			_ => merged.push((first, last)),
		}
	}
	merged
}
