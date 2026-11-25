//! The `pkg` command: where packages come from, the list of them, and
//! installing, upgrading and removing them, each answered with the text to
//! print and an exit status.
use std::{fs, path::Path};

use super::{
	index::{Package, allowed, archive_url, parse},
	store::Store,
};

/// Fetches a URL's bytes.
pub(crate) type Fetch<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;

const USAGE: &str = "\
usage: pkg <command> [name...]

  install NAME...  download and install packages, or newer versions of them
  remove NAME...   remove installed packages
  upgrade          install newer versions of every installed package
  list             every package, marking those installed
  search WORD      packages whose name or summary has WORD
  update           fetch the list of packages again
  source [URL]     show or set where packages come from
  source default   go back to Poopterm's own packages

Bundled commands work without JIT. On an iPhone or iPad, downloaded packages
need JIT: launch Poopterm from StikDebug's app list, then run `jit enable`.
";

const NO_SOURCE: &str = "pkg: no package source; set one with pkg source URL\n";

/// Runs `pkg` with `args` on the packages under `root`; `source`, from the
/// environment, overrides the one saved there, which overrides `default`
/// (Poopterm's own packages for this platform, `pkg::source`).
pub(crate) fn run(
	args: &[String],
	root: &Path,
	source: Option<&str>,
	default: Option<&str>,
	fetch: Fetch,
) -> (String, i32) {
	let saved = fs::read_to_string(root.join("source")).ok();
	let source = source
		.map(str::to_owned)
		.or(saved.map(|text| text.trim().to_owned()))
		.or(default.map(str::to_owned));
	let mut pkg = Pkg {
		root,
		source,
		default,
		fetch,
		out: String::new(),
	};
	let Some((command, names)) = args.split_first() else {
		return (USAGE.into(), 0);
	};
	let status = match command.as_str() {
		"help" | "--help" | "-h" => {
			pkg.out.push_str(USAGE);
			0
		}
		"source" => pkg.set_source(names.first()),
		"update" => pkg.update().map_or(1, |packages| {
			let plural = if packages.len() == 1 { "" } else { "s" };
			pkg
				.out
				.push_str(&format!("{} package{plural} available\n", packages.len()));
			0
		}),
		"list" => pkg.list(None),
		"search" => match names.first() {
			Some(word) => pkg.list(Some(word)),
			None => usage(&mut pkg.out),
		},
		"install" if names.is_empty() => usage(&mut pkg.out),
		"install" => pkg.install(names),
		"upgrade" => pkg.upgrade(),
		"remove" if names.is_empty() => usage(&mut pkg.out),
		"remove" => pkg.remove(names),
		other => {
			pkg.out = format!("pkg: unknown command {other}\n{USAGE}");
			2
		}
	};
	(pkg.out, status)
}

fn usage(out: &mut String) -> i32 {
	out.push_str(USAGE);
	2
}

struct Pkg<'a> {
	root: &'a Path,
	source: Option<String>,
	default: Option<&'a str>,
	fetch: Fetch<'a>,
	out: String,
}

impl Pkg<'_> {
	fn fail(&mut self, error: impl std::fmt::Display) -> i32 {
		self.out.push_str(&format!("pkg: {error}\n"));
		1
	}

	fn store(&self) -> Store {
		Store::new(self.root.to_path_buf())
	}

	fn set_source(&mut self, url: Option<&String>) -> i32 {
		let Some(url) = url else {
			return match self.source.clone() {
				Some(source) => {
					let note = if self.default == Some(source.as_str()) {
						" (the default)"
					} else {
						""
					};
					self.out.push_str(&format!("{source}{note}\n"));
					0
				}
				None => {
					self.out.push_str(NO_SOURCE);
					1
				}
			};
		};
		if url == "default" {
			return self.reset_source();
		}
		if !allowed(url) {
			return self.fail(format!(
				"{url}: packages come only over https, from files, or from this machine"
			));
		}
		let saved =
			fs::create_dir_all(self.root).and_then(|()| fs::write(self.root.join("source"), url));
		match saved {
			Ok(()) => {
				self
					.out
					.push_str(&format!("packages now come from {url}\n"));
				0
			}
			Err(error) => self.fail(error),
		}
	}

	/// Forgets the source the user set, for the default.
	fn reset_source(&mut self) -> i32 {
		let Some(default) = self.default else {
			return self.fail("this platform has no packages of Poopterm's own");
		};
		match fs::remove_file(self.root.join("source")) {
			Err(error) if error.kind() != std::io::ErrorKind::NotFound => self.fail(error),
			_ => {
				self
					.out
					.push_str(&format!("packages now come from {default}\n"));
				0
			}
		}
	}

	/// Fetches the index from the source and keeps it; its packages.
	fn update(&mut self) -> Option<Vec<Package>> {
		let Some(source) = self.source.clone() else {
			self.out.push_str(NO_SOURCE);
			return None;
		};
		let fetched = (self.fetch)(&source).and_then(|bytes| {
			let text = String::from_utf8(bytes).map_err(|_| "the index is not text".to_owned())?;
			let packages = parse(&text).map_err(|error| format!("{source}: {error}"))?;
			fs::create_dir_all(self.root).map_err(|error| error.to_string())?;
			fs::write(self.root.join("index"), text).map_err(|error| error.to_string())?;
			Ok(packages)
		});
		fetched.map_err(|error| self.fail(error)).ok()
	}

	/// The index fetched last, if there is one.
	fn cached(&self) -> Option<Vec<Package>> {
		parse(&fs::read_to_string(self.root.join("index")).ok()?).ok()
	}

	fn list(&mut self, word: Option<&String>) -> i32 {
		let Some(packages) = self.cached() else {
			return self.fail("no package list yet; run pkg update");
		};
		let installed = self.store().installed();
		let word = word.map(|word| word.to_lowercase());
		let matches = |package: &Package| {
			word.as_ref().is_none_or(|word| {
				package.name.contains(word.as_str()) || package.summary.to_lowercase().contains(word)
			})
		};
		let shown: Vec<_> = packages.iter().filter(|package| matches(package)).collect();
		let width = shown
			.iter()
			.map(|package| package.name.len())
			.max()
			.unwrap_or(0);
		for package in &shown {
			let mark = match installed.iter().find(|(name, _)| *name == package.name) {
				Some((_, version)) if *version == package.version => " [installed]".to_owned(),
				Some((_, version)) => format!(" [installed {version}]"),
				None => String::new(),
			};
			let line = format!(
				"{:width$} {} {}{mark}",
				package.name, package.version, package.summary
			);
			self.out.push_str(line.trim_end());
			self.out.push('\n');
		}
		match (word, shown.is_empty()) {
			(Some(word), true) => self.fail(format!("nothing matches {word}")),
			_ => 0,
		}
	}

	fn install(&mut self, names: &[String]) -> i32 {
		let Some(packages) = self.update_quietly() else {
			return 1;
		};
		let mut status = 0;
		for name in names {
			match packages.iter().find(|package| package.name == *name) {
				Some(package) => status |= self.install_one(package),
				None => status |= self.fail(format!("{name}: no such package")),
			}
		}
		status
	}

	fn upgrade(&mut self) -> i32 {
		let Some(packages) = self.update_quietly() else {
			return 1;
		};
		// What is installed and differs from the index: a new version, or
		// the same one rebuilt.
		let store = self.store();
		let installed = store.installed();
		let newer: Vec<_> = packages
			.iter()
			.filter(|package| {
				installed.iter().any(|(name, _)| *name == package.name) && !store.is_current(package)
			})
			.collect();
		if newer.is_empty() {
			self.out.push_str("everything is up to date\n");
		}
		newer
			.into_iter()
			.fold(0, |status, package| status | self.install_one(package))
	}

	/// The index fetched afresh, printing nothing but errors.
	fn update_quietly(&mut self) -> Option<Vec<Package>> {
		self.update()
	}

	fn install_one(&mut self, package: &Package) -> i32 {
		let store = self.store();
		// The same archive is already in place; a rebuild of the same
		// version is not.
		if store.is_current(package) {
			let (name, version) = (&package.name, &package.version);
			self
				.out
				.push_str(&format!("{name} {version} is already installed\n"));
			return 0;
		}
		let source = self.source.clone().unwrap_or_default();
		let url = archive_url(&source, &package.archive);
		let done = (self.fetch)(&url).and_then(|bytes| store.install(package, &bytes));
		match done {
			Ok(commands) => {
				let (name, version) = (&package.name, &package.version);
				let commands = commands.join(" ");
				self
					.out
					.push_str(&format!("installed {name} {version}: {commands}\n"));
				0
			}
			Err(error) => self.fail(error),
		}
	}

	fn remove(&mut self, names: &[String]) -> i32 {
		let store = self.store();
		let mut status = 0;
		for name in names {
			match store.remove(name) {
				Ok(_) => self.out.push_str(&format!("removed {name}\n")),
				Err(error) => status |= self.fail(error),
			}
		}
		status
	}
}
