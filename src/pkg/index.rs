//! The package index: one line per package (name, version, SHA-256 of its
//! archive, size, archive path and a summary), and where archives and
//! indexes may be fetched from.

/// A package as the index lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package {
	pub name: String,
	pub version: String,
	pub sha256: [u8; 32],
	pub size: u64,
	/// The archive's URL, or its path beside the index.
	pub archive: String,
	pub summary: String,
}

/// The packages `text` lists, by name, or the first line that is wrong.
pub(crate) fn parse(text: &str) -> Result<Vec<Package>, String> {
	let mut packages: Vec<Package> = Vec::new();
	for (number, line) in text.lines().enumerate() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let package = package(line).map_err(|error| format!("line {}: {error}", number + 1))?;
		if packages.iter().any(|known| known.name == package.name) {
			return Err(format!(
				"line {}: {} is listed twice",
				number + 1,
				package.name
			));
		}
		packages.push(package);
	}
	packages.sort_by(|a, b| a.name.cmp(&b.name));
	Ok(packages)
}

fn package(line: &str) -> Result<Package, String> {
	let mut fields = line
		.splitn(6, [' ', '\t'])
		.filter(|field| !field.is_empty());
	let mut next = |what: &str| fields.next().ok_or(format!("the {what} is missing"));
	let (name, version, hash, size, archive) = (
		next("name")?,
		next("version")?,
		next("hash")?,
		next("size")?,
		next("archive")?,
	);
	let summary = fields.next().unwrap_or("").trim().to_owned();
	if !valid_name(name) {
		return Err(format!("{name:?} is not a package name"));
	}
	let sha256 = digest(hash).ok_or(format!("{hash:?} is not a SHA-256 hash"))?;
	let size = size
		.parse()
		.map_err(|_| format!("{size:?} is not a size"))?;
	Ok(Package {
		name: name.into(),
		version: version.into(),
		sha256,
		size,
		archive: archive.into(),
		summary,
	})
}

/// 64 hexadecimal digits as 32 bytes.
fn digest(hex: &str) -> Option<[u8; 32]> {
	let bytes = hex.as_bytes();
	if bytes.len() != 64 {
		return None;
	}
	let mut out = [0u8; 32];
	for (byte, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
		*byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
	}
	Some(out)
}

/// A package name: lower-case letters, digits and `+ . _ -`, starting with
/// a letter or digit, so it is a safe file name.
pub(crate) fn valid_name(name: &str) -> bool {
	let mut chars = name.chars();
	let first = chars
		.next()
		.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
	let rest = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+._-".contains(c));
	first && rest && name.len() <= 64
}

/// Where `archive` is: itself if it is a URL, else beside the index.
pub(crate) fn archive_url(index: &str, archive: &str) -> String {
	if archive.contains("://") {
		return archive.into();
	}
	let directory = index.rfind('/').map_or(index, |end| &index[..=end]);
	format!("{directory}{archive}")
}

/// Whether packages may come from `url`: over HTTPS, from a file, or from
/// this machine (for building packages).
pub(crate) fn allowed(url: &str) -> bool {
	if url.starts_with("https://") || url.starts_with("file:///") {
		return true;
	}
	let Some(rest) = url.strip_prefix("http://") else {
		return false;
	};
	let host = rest.split(['/', ':']).next().unwrap_or("");
	host == "localhost" || host == "127.0.0.1"
}
