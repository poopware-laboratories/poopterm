//! The fake libwayland-client and compositor for tests outside
//! `present::wayland`, as the app's: a client and a surface on a fresh
//! compositor, what it was shown, and the ways it can fail.
use super::{
	client::{Client, Proxy},
	fake_tests::{DISPLAY, Kind, client, install, with},
};

/// The fake's `wl_display`.
pub(crate) const FAKE_DISPLAY: Proxy = DISPLAY;

/// A fresh compositor, and a client of it.
pub(crate) fn fake_client() -> Client {
	install();
	client()
}

/// A `wl_surface` of `version` on the compositor, as the window system
/// makes it.
pub(crate) fn fake_surface(version: u32) -> Proxy {
	with(|wayland| wayland.surface(version))
}

/// Each frame committed so far: its size and pixels, row by row.
pub(crate) fn fake_shown() -> Vec<([usize; 2], Vec<u32>)> {
	let commits = with(|wayland| wayland.commits.clone());
	commits
		.into_iter()
		.map(|commit| (commit.size, commit.pixels))
		.collect()
}

/// The damage of each frame committed so far: `[x, y, width, height]`.
pub(crate) fn fake_damage() -> Vec<Vec<[i32; 4]>> {
	let commits = with(|wayland| wayland.commits.clone());
	let rectangles = |commit: super::fake_tests::Commit| {
		commit
			.damage
			.into_iter()
			.map(|(_, rectangle)| rectangle)
			.collect()
	};
	commits.into_iter().map(rectangles).collect()
}

/// The compositor lets go of every buffer it holds.
pub(crate) fn fake_release_all() {
	with(|wayland| wayland.release_all());
}

/// The compositor offers `wl_shm`, but not our pixel format.
pub(crate) fn fake_without_xrgb() {
	with(|wayland| {
		wayland
			.formats
			.retain(|&format| format != super::events::XRGB8888)
	});
}

/// The connection to the compositor breaks.
pub(crate) fn fake_lose() {
	with(|wayland| wayland.lost = true);
}

/// Making `wl_shm_pool`s fails, as when libwayland runs out of memory.
pub(crate) fn fake_refuse_pools() {
	with(|wayland| wayland.refuse.push((Kind::Shm, 0)));
}

/// The rules of the protocol or of libwayland broken so far.
pub(crate) fn fake_broken() -> Vec<String> {
	with(|wayland| wayland.broken.clone())
}

/// How many of our objects are left undestroyed, surfaces aside.
pub(crate) fn fake_leaked() -> usize {
	with(|wayland| wayland.leaked().len())
}

/// Runs `f` with the display and a surface of a real compositor, then
/// requires that no protocol error happened: `None` when there is none to
/// run on (see `real_tests`).
pub(crate) fn on_real_surface<R>(f: impl FnOnce(Proxy, Proxy) -> R) -> Option<R> {
	super::real_tests::on_real_surface(f)
}
