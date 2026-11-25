//! `pkg`, the package manager of the in-process shell: a signed-by-hash
//! index of packages, each a `tar.gz` of command libraries, downloaded by
//! the system's HTTP stack, checked, and unpacked under the user's
//! `Library/poopterm/packages`, whose `bin` is on `$PATH`.
use std::path::{Path, PathBuf};

pub(crate) mod command;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod download;
pub(crate) mod fetch;
mod index;
mod sha256;
pub(crate) mod source;
mod store;
mod tar;

/// The most an index or an archive may be.
pub(crate) const DOWNLOAD_LIMIT: usize = 256 << 20;

/// Where packages are installed for the user whose home is `home`.
pub(crate) fn root(home: &Path) -> PathBuf {
	home.join("Library/poopterm/packages")
}

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod fetch_tests;
#[cfg(test)]
mod index_tests;
#[cfg(test)]
mod sha256_tests;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tar_tests;
