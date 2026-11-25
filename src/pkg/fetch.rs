//! Fetching an index or an archive: files read from disk, anything else
//! downloaded by the system's own HTTP stack, from the places packages may
//! come from only, and no larger than a limit.
use super::index::allowed;

/// The bytes at `url`, at most `limit` of them.
pub(crate) fn fetch(url: &str, limit: usize) -> Result<Vec<u8>, String> {
	if !allowed(url) {
		return Err(format!(
			"{url}: packages come only over https, from files, or from this machine"
		));
	}
	let bytes = match url.strip_prefix("file://") {
		Some(path) => {
			let size = std::fs::metadata(path)
				.map_err(|error| format!("{path}: {error}"))?
				.len();
			if size > limit as u64 {
				return Err(format!("{url}: larger than {limit} bytes"));
			}
			std::fs::read(path).map_err(|error| format!("{path}: {error}"))?
		}
		None => download(url, limit)?,
	};
	if bytes.len() > limit {
		return Err(format!("{url}: larger than {limit} bytes"));
	}
	Ok(bytes)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn download(url: &str, limit: usize) -> Result<Vec<u8>, String> {
	super::download::get(url, limit)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn download(url: &str, _: usize) -> Result<Vec<u8>, String> {
	Err(format!("{url}: downloading packages needs macOS or iOS"))
}
