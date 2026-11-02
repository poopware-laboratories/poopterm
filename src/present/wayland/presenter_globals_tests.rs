//! Which globals the compositor offers, as the presenter saw them while
//! binding `wl_shm`: how the window learns whether anyone else draws its
//! title bar.
use super::{
	fake_tests::{DISPLAY, client, install, with},
	presenter::Presenter,
};

const DECORATIONS: &[u8] = b"zxdg_decoration_manager_v1";

fn presenter_on(globals: &[(u32, &'static std::ffi::CStr, u32)]) -> Presenter {
	install();
	let surface = with(|wayland| {
		wayland.globals.extend_from_slice(globals);
		wayland.surface(6)
	});
	// SAFETY: the fake's display and surface live as long as the thread.
	unsafe { Presenter::new(client(), DISPLAY, surface, [8, 6]) }.expect("a presenter")
}

#[test]
fn a_compositor_without_server_side_decorations_does_not_offer_them() {
	// GNOME's mutter offers no decoration manager.
	let mut presenter = presenter_on(&[]);
	assert!(!presenter.offers(DECORATIONS));
	assert!(presenter.offers(b"wl_shm"));
	assert!(presenter.offers(b"xdg_wm_base"));
}

#[test]
fn a_compositor_with_the_decoration_manager_offers_it() {
	let mut presenter = presenter_on(&[(9, c"zxdg_decoration_manager_v1", 1)]);
	assert!(presenter.offers(DECORATIONS));
}

#[test]
fn names_match_whole_and_exactly() {
	let mut presenter = presenter_on(&[(9, c"zxdg_decoration_manager_v1", 1)]);
	for near in [
		&b"zxdg_decoration_manager"[..],
		b"zxdg_decoration_manager_v12",
		b"ZXDG_DECORATION_MANAGER_V1",
		b"",
	] {
		assert!(!presenter.offers(near), "{}", String::from_utf8_lossy(near));
	}
}

#[test]
fn asking_and_dropping_leaves_nothing_behind() {
	// The window opens a presenter only to ask, then drops it.
	let mut presenter = presenter_on(&[(9, c"zxdg_decoration_manager_v1", 1)]);
	assert!(presenter.offers(DECORATIONS));
	drop(presenter);
	assert_eq!(with(|wayland| wayland.leaked()), []);
	assert!(with(|wayland| wayland.queues.values().all(|live| !live)));
	let broken = with(|wayland| wayland.broken.clone());
	assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
fn asking_twice_gives_the_same_answer_and_sends_nothing() {
	let mut presenter = presenter_on(&[]);
	let sent = with(|wayland| wayland.objects.len());
	assert!(!presenter.offers(DECORATIONS));
	assert!(!presenter.offers(DECORATIONS));
	assert_eq!(with(|wayland| wayland.objects.len()), sent);
}
