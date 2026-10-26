//! Tests of the Wayland presenter's edges: requests libwayland could not
//! make an object for, frames whose pixels fall short, windows that keep
//! their area but change shape, and presenters sharing one connection.
use super::{
	client::Proxy,
	fake_tests::{DISPLAY, Kind, Wayland, client, install, with},
	presenter::{Error, Presented, Presenter},
};

const SIZE: [u32; 2] = [8, 6];

fn frame(size: [u32; 2], number: u32) -> Vec<u32> {
	(0..size[0] * size[1])
		.map(|index| number << 16 | index)
		.collect()
}

/// A presenter on the current compositor, with a new surface.
fn presenter() -> Result<Presenter, Error> {
	let surface = with(|wayland| wayland.surface(6));
	// SAFETY: the fake's display and surface live as long as the thread.
	unsafe { Presenter::new(client(), DISPLAY, surface, SIZE) }
}

fn rules_kept() {
	let broken = with(|wayland| wayland.broken.clone());
	assert!(broken.is_empty(), "{broken:#?}");
}

fn nothing_left() {
	assert_eq!(with(|wayland| wayland.leaked()), []);
	assert!(with(|wayland| wayland.queues.values().all(|live| !live)));
}

fn last_pixels() -> Vec<u32> {
	with(|wayland| wayland.commits.last().expect("a commit").pixels.clone())
}

#[test]
fn a_wrapper_or_registry_libwayland_cannot_make_loses_the_connection() {
	for refused in [(Kind::Wrapper, 0), (Kind::Wrapper, 1)] {
		install();
		with(|wayland| wayland.refuse.push(refused));
		assert_eq!(presenter().err(), Some(Error::Lost), "{refused:?}");
		nothing_left();
		rules_kept();
	}
}

#[test]
fn a_wl_shm_libwayland_cannot_bind_is_as_none_offered() {
	install();
	with(|wayland| wayland.refuse.push((Kind::Registry, 0)));
	assert_eq!(presenter().err(), Some(Error::NoShm));
	nothing_left();
	rules_kept();
}

#[test]
fn a_pool_or_buffer_that_cannot_be_made_fails_the_frame_and_leaves_nothing() {
	for refused in [(Kind::Shm, 0), (Kind::Pool, 0)] {
		install();
		let mut presenter = presenter().unwrap();
		with(|wayland| wayland.refuse.push(refused));
		assert_eq!(
			presenter.present(&frame(SIZE, 1), SIZE, None),
			Err(Error::Lost)
		);
		// Only wl_shm is left: a pool made for a buffer that was not is gone.
		let left: Vec<_> = with(|wayland| wayland.leaked())
			.into_iter()
			.map(|(_, kind)| kind)
			.collect();
		assert_eq!(left, [Kind::Shm], "{refused:?}");
		// Once libwayland makes them again, the frame shows whole.
		with(|wayland| wayland.refuse.clear());
		let pixels = frame(SIZE, 2);
		assert_eq!(
			presenter.present(&pixels, SIZE, Some(&[])),
			Ok(Presented::Shown)
		);
		assert_eq!(last_pixels(), pixels);
		drop(presenter);
		nothing_left();
		rules_kept();
	}
}

#[test]
fn pixels_short_of_the_frame_fill_what_they_cover_and_leave_the_rest() {
	install();
	let mut presenter = presenter().unwrap();
	// Two and a half rows of a six-row frame.
	let pixels = frame(SIZE, 1)[..20].to_vec();
	assert_eq!(presenter.present(&pixels, SIZE, None), Ok(Presented::Shown));
	let shown = last_pixels();
	assert_eq!(shown[..20], pixels[..]);
	assert!(
		shown[20..].iter().all(|&pixel| pixel == 0),
		"a new buffer is zeroed"
	);
	rules_kept();
}

#[test]
fn a_window_of_the_same_area_but_another_shape_gets_new_buffers() {
	install();
	let mut presenter = presenter().unwrap();
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	with(|wayland| wayland.release_all());
	let turned = [6, 8];
	let pixels = frame(turned, 2);
	assert_eq!(
		presenter.present(&pixels, turned, Some(&[])),
		Ok(Presented::Shown)
	);
	let commit = with(|wayland| wayland.commits.last().cloned().unwrap());
	assert_eq!(commit.size, [6, 8]);
	assert_eq!(commit.pixels, pixels);
	let shape = with(|wayland| wayland.objects[&commit.buffer].shape.unwrap().1);
	assert_eq!(shape, [0, 6, 8, 24]);
	assert_eq!(commit.damage, [(9, [0, 0, 6, 8])]);
	rules_kept();
}

#[test]
fn presenters_on_one_connection_keep_to_their_own_queues() {
	install();
	let mut first = presenter().unwrap();
	let mut second = presenter().unwrap();
	first.present(&frame(SIZE, 1), SIZE, None).unwrap();
	second.present(&frame(SIZE, 2), SIZE, None).unwrap();
	let buffers: Vec<usize> = with(|wayland| wayland.held());
	let queue_of = |buffer: usize| with(|wayland| wayland.objects[&buffer].queue);
	assert_ne!(queue_of(buffers[0]), queue_of(buffers[1]));
	// Releasing the first presenter's buffer frees it for the first alone:
	// the second, still holding its only buffer, makes another.
	with(|wayland| wayland.release(buffers[0]));
	second.present(&frame(SIZE, 3), SIZE, None).unwrap();
	first.present(&frame(SIZE, 4), SIZE, None).unwrap();
	let commits: Vec<usize> =
		with(|wayland| wayland.commits.iter().map(|commit| commit.buffer).collect());
	assert_eq!(commits[2..], [commits[2], buffers[0]]);
	assert!(!buffers.contains(&commits[2]));
	drop(first);
	drop(second);
	nothing_left();
	rules_kept();
}

#[test]
fn formats_and_globals_in_any_order_are_found() {
	install();
	with(|wayland: &mut Wayland| {
		wayland.globals.reverse();
		wayland.globals.insert(0, (40, c"wl_shm_like", 9));
		wayland.formats = vec![0x3432_5258, 0, 1, 0x3034_5241];
	});
	let mut presenter = presenter().unwrap();
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	assert_eq!(last_pixels(), frame(SIZE, 1));
	drop(presenter);
	rules_kept();
}

#[test]
fn a_surface_proxy_is_never_used_after_the_presenter_drops() {
	install();
	let mut presenter = presenter().unwrap();
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	let requests = with(|wayland| wayland.requests);
	let surfaces: Vec<Proxy> = with(|wayland| {
		let surfaces = wayland
			.objects
			.iter()
			.filter(|(_, object)| object.kind == Kind::Surface);
		surfaces.map(|(&id, _)| id as Proxy).collect()
	});
	drop(presenter);
	// Dropping sends only destructors: two per buffer, and wl_shm's release.
	assert_eq!(with(|wayland| wayland.requests) - requests, 3);
	assert!(with(|wayland| surfaces
		.iter()
		.all(|&surface| wayland.objects[&(surface as usize)].live)));
	rules_kept();
}
