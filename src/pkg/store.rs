//! Installed packages, in a directory of their own: each package unpacked
//! in `<root>/<name>`, its commands linked from `<root>/bin` (on `$PATH`),
//! and `<root>/installed` naming each package and its version. An archive
//! is checked against its index entry, then unpacked beside the old
//! version and swapped in, so a failed install leaves the old one whole.
use std::{fs, path::PathBuf};

use super::{
	index::{Package, valid_name},
	sha256::{hex, sha256},
	tar::{Kind, entries},
};

/// The most an archive may unpack to.
const UNPACKED_LIMIT: usize = 512 << 20;

pub(crate) struct Store {
	root: PathBuf,
}

impl Store {
	pub(crate) fn new(root: PathBuf) -> Self {
		Self { root }
	}

	/// The installed packages and their versions, by name.
	pub(crate) fn installed(&self) -> Vec<(String, String)> {
		let text = fs::read_to_string(self.root.join("installed")).unwrap_or_default();
		let mut list: Vec<(String, String)> = text
			.lines()
			.filter_map(|line| line.split_once(' '))
			.map(|(name, version)| (name.to_owned(), version.to_owned()))
			.collect();
		list.sort();
		list
	}

	fn record(&self, list: &[(String, String)]) -> Result<(), String> {
		let text: String = list
			.iter()
			.map(|(name, version)| format!("{name} {version}\n"))
			.collect();
		let path = self.root.join("installed");
		fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
	}

	/// Installs `archive` as `package`, replacing any version installed;
	/// the commands it put on the path.
	pub(crate) fn install(&self, package: &Package, archive: &[u8]) -> Result<Vec<String>, String> {
		let name = package.name.as_str();
		if !valid_name(name) {
			return Err(format!("{name:?} is not a package name"));
		}
		if archive.len() as u64 != package.size {
			let (got, listed) = (archive.len(), package.size);
			return Err(format!(
				"{name}: the archive's size is {got}, the index says {listed}"
			));
		}
		let digest = sha256(archive);
		if digest != package.sha256 {
			let (got, listed) = (hex(&digest), hex(&package.sha256));
			return Err(format!(
				"{name}: the archive's SHA-256 is {got}, the index says {listed}"
			));
		}
		let tar = crate::inflate::gzip::gzip(archive, UNPACKED_LIMIT)
			.ok_or(format!("{name}: the archive is not gzip data"))?;
		let files = entries(&tar).map_err(|error| format!("{name}: {error}"))?;
		let mut commands = Vec::new();
		for entry in &files {
			let Some(command) = entry.path.strip_prefix("bin/") else {
				continue;
			};
			if command.contains('/') {
				return Err(format!(
					"{name}: commands go directly in bin/, not {}",
					entry.path
				));
			}
			if matches!(entry.kind, Kind::File { .. }) {
				commands.push(command.to_owned());
			}
		}
		if commands.is_empty() {
			return Err(format!("{name}: the package has no commands in bin/"));
		}
		for command in &commands {
			match self.owner(command) {
				Some(owner) if owner != name => {
					return Err(format!("{name}: {command} belongs to {owner}"));
				}
				_ => {}
			}
		}
		let io = |error: std::io::Error| format!("{name}: {error}");
		let staging = self.root.join(format!(".{name}.new"));
		let _ = fs::remove_dir_all(&staging);
		for entry in &files {
			let path = staging.join(&entry.path);
			match entry.kind {
				Kind::Directory => fs::create_dir_all(&path).map_err(io)?,
				Kind::File { executable } => {
					if let Some(parent) = path.parent() {
						fs::create_dir_all(parent).map_err(io)?;
					}
					fs::write(&path, entry.data).map_err(io)?;
					set_executable(&path, executable).map_err(io)?;
				}
			}
		}
		let old = self.links_of(name);
		let home = self.root.join(name);
		if home.exists() {
			fs::remove_dir_all(&home).map_err(io)?;
		}
		fs::rename(&staging, &home).map_err(io)?;
		let bin = self.root.join("bin");
		fs::create_dir_all(&bin).map_err(io)?;
		for command in old.iter().filter(|command| !commands.contains(command)) {
			let _ = fs::remove_file(bin.join(command));
		}
		for command in &commands {
			let link = bin.join(command);
			let _ = fs::remove_file(&link);
			link_to(&format!("../{name}/bin/{command}"), &link).map_err(io)?;
		}
		let mut list = self.installed();
		list.retain(|(installed, _)| installed != name);
		list.push((name.to_owned(), package.version.clone()));
		self.record(&list)?;
		Ok(commands)
	}

	/// Removes package `name`; the commands it took off the path.
	pub(crate) fn remove(&self, name: &str) -> Result<Vec<String>, String> {
		let mut list = self.installed();
		if !valid_name(name) || !list.iter().any(|(installed, _)| installed == name) {
			return Err(format!("{name}: not installed"));
		}
		let commands = self.links_of(name);
		for command in &commands {
			let _ = fs::remove_file(self.root.join("bin").join(command));
		}
		let home = self.root.join(name);
		fs::remove_dir_all(&home).map_err(|error| format!("{}: {error}", home.display()))?;
		list.retain(|(installed, _)| installed != name);
		self.record(&list)?;
		Ok(commands)
	}

	/// The package whose command `command` is on the path: the link's
	/// target names it; something that is not one of our links has no name.
	fn owner(&self, command: &str) -> Option<String> {
		let link = self.root.join("bin").join(command);
		fs::symlink_metadata(&link).ok()?;
		let target = fs::read_link(&link).ok();
		let owner = target.as_ref().and_then(|target| {
			let text = target.to_str()?;
			text
				.strip_prefix("../")?
				.split('/')
				.next()
				.map(str::to_owned)
		});
		Some(owner.unwrap_or_else(|| "a file that is not a package's".to_owned()))
	}

	/// The commands on the path that package `name` put there.
	fn links_of(&self, name: &str) -> Vec<String> {
		let Ok(dir) = fs::read_dir(self.root.join("bin")) else {
			return Vec::new();
		};
		dir
			.filter_map(|entry| entry.ok()?.file_name().into_string().ok())
			.filter(|command| self.owner(command).as_deref() == Some(name))
			.collect()
	}
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path, executable: bool) -> std::io::Result<()> {
	use std::os::unix::fs::PermissionsExt;
	let mode = if executable { 0o755 } else { 0o644 };
	fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_executable(_: &std::path::Path, _: bool) -> std::io::Result<()> {
	Ok(())
}

#[cfg(unix)]
fn link_to(target: &str, link: &std::path::Path) -> std::io::Result<()> {
	std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn link_to(target: &str, link: &std::path::Path) -> std::io::Result<()> {
	let from = link.parent().unwrap_or(link).join(target);
	fs::copy(from, link).map(drop)
}
