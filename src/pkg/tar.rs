//! A reader for the tar archives packages come in (POSIX ustar, with pax
//! long names or GNU's, as Linux's tar writes them), strict about what a package may hold: regular files and
//! directories only, at relative paths that stay inside the package.

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
	File { executable: bool },
	Directory,
}

/// One entry: its path inside the package, without `./` or a final `/`.
#[derive(Debug)]
pub(crate) struct Entry<'a> {
	pub path: String,
	pub kind: Kind,
	pub data: &'a [u8],
}

const BLOCK: usize = 512;
/// More entries than any package needs.
const MAX_ENTRIES: usize = 100_000;

/// The entries of `archive`, or what is wrong with it.
pub(crate) fn entries(archive: &[u8]) -> Result<Vec<Entry<'_>>, String> {
	let mut out = Vec::new();
	let mut at = 0;
	let mut long_path: Option<String> = None;
	while at < archive.len() {
		let header: &[u8; BLOCK] = (archive.get(at..at + BLOCK))
			.and_then(|block| block.try_into().ok())
			.ok_or("the archive ends inside a header")?;
		if header.iter().all(|&byte| byte == 0) {
			break;
		}
		let name = name(header)?;
		if checksum(header) != Some(octal(&header[148..156]).ok_or("a bad checksum field")?) {
			return Err(format!("{name}: the header's checksum is wrong"));
		}
		let size = octal(&header[124..136]).ok_or_else(|| format!("{name}: a bad size"))?;
		let start = at + BLOCK;
		let data = usize::try_from(size)
			.ok()
			.and_then(|size| archive.get(start..start.checked_add(size)?))
			.ok_or_else(|| format!("{name}: the archive ends inside its data"))?;
		at = start + data.len().div_ceil(BLOCK) * BLOCK;
		let kind = match header[156] {
			b'x' => {
				long_path = pax_path(data);
				continue;
			}
			b'g' => continue,
			b'L' => {
				long_path = Some(field(data)?.to_owned());
				continue;
			}
			b'0' | 0 => {
				let mode = octal(&header[100..108]).unwrap_or(0);
				Kind::File {
					executable: mode & 0o111 != 0,
				}
			}
			b'5' => Kind::Directory,
			other => {
				let what = if matches!(other, b'1' | b'2') {
					"a link"
				} else {
					"not a file"
				};
				return Err(format!("{name}: {what}, which packages may not hold"));
			}
		};
		let path = clean(&long_path.take().unwrap_or(name))?;
		if path.is_empty() {
			continue;
		}
		if out.len() == MAX_ENTRIES {
			return Err("too many entries".into());
		}
		out.push(Entry { path, kind, data });
	}
	Ok(out)
}

/// The header's name, its ustar prefix joined in front.
fn name(header: &[u8; BLOCK]) -> Result<String, String> {
	let (name, prefix) = (field(&header[..100])?, field(&header[345..500])?);
	// POSIX's magic, not GNU's "ustar ", whose prefix bytes hold other things.
	let ustar = &header[257..263] == b"ustar\0";
	Ok(if ustar && !prefix.is_empty() {
		format!("{prefix}/{name}")
	} else {
		name.to_owned()
	})
}

/// A NUL-padded text field.
fn field(bytes: &[u8]) -> Result<&str, String> {
	let end = (bytes.iter().position(|&byte| byte == 0)).unwrap_or(bytes.len());
	std::str::from_utf8(&bytes[..end]).map_err(|_| "a name that is not UTF-8".to_owned())
}

/// The sum of the header's bytes, its checksum field counted as spaces.
fn checksum(header: &[u8; BLOCK]) -> Option<u64> {
	let sum: u64 = header.iter().map(|&byte| u64::from(byte)).sum();
	let field: u64 = header[148..156].iter().map(|&byte| u64::from(byte)).sum();
	Some(sum - field + 8 * u64::from(b' '))
}

/// An octal number field, padded with spaces or NULs.
fn octal(field: &[u8]) -> Option<u64> {
	let text = std::str::from_utf8(field).ok()?;
	let digits = text.trim_matches(|c: char| c == ' ' || c == '\0');
	if digits.is_empty() {
		return Some(0);
	}
	u64::from_str_radix(digits, 8).ok()
}

/// The `path` record of a pax header, if it has one.
fn pax_path(data: &[u8]) -> Option<String> {
	let text = std::str::from_utf8(data).ok()?;
	text.lines().find_map(|record| {
		let (_, pair) = record.split_once(' ')?;
		pair.strip_prefix("path=").map(str::to_owned)
	})
}

/// `path` without `./` steps or a final `/`; refused if absolute or if a
/// step climbs out.
fn clean(path: &str) -> Result<String, String> {
	if path.starts_with('/') {
		return Err(format!("{path}: an absolute path"));
	}
	let mut steps = Vec::new();
	for step in path.split('/') {
		match step {
			"" | "." => {}
			".." => return Err(format!("{path}: a path that climbs out of the package")),
			_ => steps.push(step),
		}
	}
	Ok(steps.join("/"))
}
