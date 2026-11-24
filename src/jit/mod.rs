//! Running programs iOS did not sign: once a debugger has given the app
//! JIT (StikDebug, `jit enable`), a command library downloaded by `pkg`
//! is loaded by our own loader into memory the debugger made executable,
//! instead of by dyld, which refuses unsigned code.
pub(crate) mod loader;
pub(crate) mod macho;

use std::{
	collections::HashMap,
	ffi::{CString, c_void},
	path::{Path, PathBuf},
	ptr::NonNull,
	sync::{Arc, LazyLock, Mutex, MutexGuard},
	time::{Duration, Instant, SystemTime},
};

use crate::sys::{dl, jit::Arena};

/// How code we load can be made executable here and now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Strategy {
	/// Map it writable, then executable: macOS, the simulator, and iOS
	/// before 26 with a debugger attached.
	Plain,
	/// Into the region a debugger prepared (iOS 26 and later).
	Prepared,
	/// Not at all yet, and why.
	Unavailable(&'static str),
}

pub(crate) const NEEDS_JIT: &str = "unsigned code needs JIT: launch Poopterm from StikDebug's app list (it lists apps signed with get-task-allow)";
pub(crate) const NEEDS_SCRIPT: &str = "JIT is on, but iOS 26 and later also need a region StikDebug's universal script prepares: choose that script for Poopterm in StikDebug, relaunch from there, then run `jit enable`";

/// The strategy for a device (`ios`: a real iPhone or iPad, not the
/// simulator) on system version `major`, with a debugger attached or not
/// and a region prepared or not.
pub(crate) fn strategy(ios: bool, major: u32, debugged: bool, prepared: bool) -> Strategy {
	match (ios, debugged, major >= 26) {
		(false, _, _) => Strategy::Plain,
		(true, false, _) => Strategy::Unavailable(NEEDS_JIT),
		(true, true, false) => Strategy::Plain,
		(true, true, true) if prepared => Strategy::Prepared,
		(true, true, true) => Strategy::Unavailable(NEEDS_SCRIPT),
	}
}

/// Whether this is a real iOS device (where only signed code runs).
pub(crate) const DEVICE: bool = cfg!(all(target_os = "ios", not(target_abi = "sim")));

/// The system's major version.
pub(crate) fn major_version() -> u32 {
	crate::sys::jit::os_version()
		.and_then(|version| version.split('.').next()?.parse().ok())
		.unwrap_or(0)
}

/// The link that asks StikDebug to attach to process `pid` of the app
/// `bundle`, with its universal script where TXM needs it.
pub(crate) fn stikdebug_url(bundle: &str, pid: u32, script: bool) -> String {
	let mut url = format!("stikdebug://enable-jit?bundle-id={bundle}&pid={pid}");
	if script {
		url.push_str("&script-name=universal.js");
	}
	url
}

/// What the app knows about its JIT.
#[derive(Default)]
struct State {
	arena: Option<Arena>,
	/// Libraries loaded, by path, with the file's time and size then.
	loaded: HashMap<PathBuf, (Option<SystemTime>, u64, Arc<loader::Loaded>)>,
}

fn state() -> MutexGuard<'static, State> {
	static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
	STATE.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// The strategy here and now.
fn current(state: &State) -> Strategy {
	let prepared = state.arena.as_ref().is_some_and(Arena::is_prepared);
	strategy(
		DEVICE,
		major_version(),
		crate::sys::jit::debugged(),
		prepared,
	)
}

/// A command library open for a run: by dyld, or by our loader.
pub(crate) enum Opened {
	Dyld(dl::Library),
	Ours(Arc<loader::Loaded>),
}

impl Opened {
	/// Its `poopterm_entry`, ready for a fresh run.
	pub(crate) fn entry(&self) -> Option<NonNull<c_void>> {
		match self {
			Self::Dyld(library) => library.entry(),
			Self::Ours(loaded) => NonNull::new(loaded.fresh_entry()),
		}
	}
}

/// Opens the command library at `path`: dyld first; when it refuses on an
/// iOS device (unsigned code), our loader, if JIT allows. Elsewhere dyld
/// runs unsigned code itself, so its refusal stands. `force` uses our
/// loader at once (`POOPTERM_LOADER=1`, for testing it).
pub(crate) fn open(path: &Path, force: bool) -> Result<Opened, String> {
	let name = CString::new(path.as_os_str().as_encoded_bytes())
		.map_err(|_| "a path with a NUL byte".to_owned())?;
	let refused = match force {
		true => String::new(),
		false => match dl::Library::open(&name) {
			Ok(library) => return Ok(Opened::Dyld(library)),
			Err(error) if !DEVICE => return Err(error),
			Err(error) => error,
		},
	};
	let mut state = state();
	let strategy = current(&state);
	if let Strategy::Unavailable(why) = strategy {
		return Err(if refused.is_empty() {
			why.to_owned()
		} else {
			format!("{refused}\n{why}")
		});
	}
	let meta = std::fs::metadata(path).map_err(|error| error.to_string())?;
	let stamp = (meta.modified().ok(), meta.len());
	if let Some((time, size, loaded)) = state.loaded.get(path)
		&& (*time, *size) == stamp
	{
		return Ok(Opened::Ours(loaded.clone()));
	}
	let arena = state.arena.get_or_insert_with(|| Arena::plain(4 << 30));
	let loaded = Arc::new(loader::load(path, arena)?);
	state
		.loaded
		.insert(path.to_owned(), (stamp.0, stamp.1, loaded.clone()));
	Ok(Opened::Ours(loaded))
}

/// The `jit` built-in: `jit` says where JIT stands, `jit enable [MB]` asks
/// StikDebug for it and prepares a region of MB megabytes (64 by default)
/// where iOS needs one. `stop` is polled while waiting: true ends the wait.
pub(crate) fn command(args: &[String], stop: &dyn Fn() -> bool) -> (String, i32) {
	match args.first().map(String::as_str) {
		None | Some("status") => (status(&state()), 0),
		Some("enable") => {
			let megabytes = match args.get(1).map(|size| size.parse::<usize>()) {
				None => 64,
				Some(Ok(size)) if (16..=1024).contains(&size) => size,
				Some(_) => return ("jit: the region is 16 to 1024 MB\n".to_owned(), 2),
			};
			enable(megabytes << 20, stop)
		}
		Some(_) => (
			"usage: jit [status]\n       jit enable [MB]   ask StikDebug for JIT\n".to_owned(),
			2,
		),
	}
}

/// Where JIT stands, for `jit status`.
fn status(state: &State) -> String {
	let mut text = match current(state) {
		Strategy::Plain if !DEVICE => {
			"JIT: not needed here; this system runs unsigned code\n".to_owned()
		}
		Strategy::Plain => "JIT: on (debugger attached)\n".to_owned(),
		Strategy::Prepared => "JIT: on (region prepared by StikDebug)\n".to_owned(),
		Strategy::Unavailable(why) => format!("JIT: off; {why}\n"),
	};
	if let Some(arena) = &state.arena {
		let (used, total) = arena.usage();
		text.push_str(&format!(
			"code loaded: {} libraries, {} of {} MB\n",
			state.loaded.len(),
			used >> 20,
			total >> 20
		));
	}
	text
}

/// `jit enable`.
fn enable(region: usize, stop: &dyn Fn() -> bool) -> (String, i32) {
	if !DEVICE {
		return (
			"jit: not needed here; this system runs unsigned code\n".to_owned(),
			0,
		);
	}
	let txm = major_version() >= 26;
	if matches!(current(&state()), Strategy::Plain | Strategy::Prepared) {
		return (status(&state()), 0);
	}
	if !crate::sys::jit::debugged() {
		let Some(bundle) = bundle_identifier() else {
			return ("jit: the app has no bundle identifier\n".to_owned(), 1);
		};
		open_url(&stikdebug_url(&bundle, std::process::id(), txm));
		let start = Instant::now();
		while !crate::sys::jit::debugged() {
			if stop() || start.elapsed() > Duration::from_secs(120) {
				return (
					"jit: StikDebug did not attach; launch Poopterm from StikDebug's app list instead\n"
						.to_owned(),
					1,
				);
			}
			std::thread::sleep(Duration::from_millis(200));
		}
	}
	if !txm {
		state().arena = Some(Arena::plain(4 << 30));
		return ("JIT: on\n".to_owned(), 0);
	}
	match prepare(region) {
		Ok(arena) => {
			state().arena = Some(arena);
			(
				format!("JIT: on, {} MB prepared for code\n", region >> 20),
				0,
			)
		}
		Err(error) => (format!("jit: {error}\n"), 1),
	}
}

/// Asks the attached debugger's script for the region, then lets it go.
#[cfg(all(target_os = "ios", target_arch = "aarch64"))]
fn prepare(region: usize) -> Result<Arena, String> {
	// SAFETY: the user ran `jit enable` with StikDebug attached (launched
	// from its menu, or through our link) and its universal script chosen
	// for this app, which answers both breakpoints.
	let start = unsafe { crate::sys::jit::prepare_region(region) };
	// SAFETY: as above.
	unsafe { crate::sys::jit::detach_debugger() };
	if start == 0 {
		return Err("StikDebug prepared no region".to_owned());
	}
	// SAFETY: the debugger just made this region executable for us alone.
	unsafe { Arena::prepared(start, region) }.map_err(|error| error.to_string())
}

#[cfg(not(all(target_os = "ios", target_arch = "aarch64")))]
fn prepare(_region: usize) -> Result<Arena, String> {
	Err("only iOS devices prepare regions".to_owned())
}

#[cfg(target_os = "ios")]
fn bundle_identifier() -> Option<String> {
	crate::ios_jit::bundle_identifier()
}

#[cfg(not(target_os = "ios"))]
fn bundle_identifier() -> Option<String> {
	None
}

#[cfg(target_os = "ios")]
fn open_url(url: &str) {
	crate::ios_jit::open_url(url);
}

#[cfg(not(target_os = "ios"))]
fn open_url(_url: &str) {}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
