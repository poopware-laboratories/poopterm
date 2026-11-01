//! Tests of the X11 presenter's `PutImage` fallback and of how it probes for
//! shared memory, against the fake server: requests hold whole rows of the
//! frame's own size, at the window's depth and within the server's maximum,
//! and a server that cannot attach memory by descriptor is never asked to.
use super::{
	client::{Client, Connection, Cookie, GenericError, VersionReply},
	fake_tests::{DISPLAY, Put, client, server, window},
	presenter::{Presented, Presenter},
};
use std::{cell::Cell, ptr};

/// A frame whose every pixel differs from those of other seeds.
fn frame(seed: u32, [width, height]: [usize; 2]) -> Vec<u32> {
	(0..width * height)
		.map(|index| seed << 24 | index as u32)
		.collect()
}

fn presenter_with(client: Client, window: u32) -> Presenter {
	// SAFETY: the fake display is live for the thread.
	unsafe { Presenter::new(client, DISPLAY, window) }.expect("a presenter")
}

/// What `window` shows once the server served every request.
fn shown(window: u32) -> Vec<u32> {
	server(|server| {
		server.serve();
		server.windows[&window].pixels.clone()
	})
}

/// The puts served since last asked.
fn puts() -> Vec<Put> {
	server(|server| {
		server.serve();
		std::mem::take(&mut server.puts)
	})
}

/// Requires that no rule was broken and nothing given to the server is left.
fn clean() {
	server(|server| {
		server.serve();
		assert_eq!(server.broken, Vec::<String>::new());
		assert!(server.gcs.is_empty(), "GCs left: {:?}", server.gcs);
		assert!(server.segments.is_empty(), "segments left");
		assert!(
			server.replies.is_empty(),
			"replies left: {:?}",
			server.replies
		);
	});
}

/// A put of `height` rows of `width` pixels at row `y`, not shared.
fn rows(width: usize, y: usize, height: usize) -> Put {
	Put {
		shared: false,
		size: [width, height],
		y,
	}
}

#[test]
fn put_image_requests_carry_the_window_depth() {
	server(|server| server.shm = None);
	// The fake's setup takes 32-bit pixels at depths 24 and 32 alike.
	let window = window(32, [3, 2]);
	let mut presenter = presenter_with(client(), window);
	let pixels = frame(1, [3, 2]);
	assert_eq!(
		presenter.present(&pixels, [3, 2], None),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), pixels);
	drop(presenter);
	clean();
}

#[test]
fn a_maximum_one_unit_short_of_three_rows_puts_two_at_a_time() {
	server(|server| {
		server.shm = None;
		// Six units of header and three rows of ten pixels, less one unit.
		server.max_units = 6 + 30 - 1;
	});
	let window = window(24, [10, 5]);
	let mut presenter = presenter_with(client(), window);
	let pixels = frame(2, [10, 5]);
	assert_eq!(
		presenter.present(&pixels, [10, 5], None),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), pixels);
	assert_eq!(puts(), [rows(10, 0, 2), rows(10, 2, 2), rows(10, 4, 1)]);
	drop(presenter);
	clean();
}

#[test]
fn a_server_that_cannot_map_our_memory_gets_requests_within_its_maximum() {
	server(|server| {
		server.refuse_attach = true;
		server.max_units = 6 + 20;
	});
	let window = window(24, [10, 3]);
	let mut presenter = presenter_with(client(), window);
	let pixels = frame(3, [10, 3]);
	assert_eq!(
		presenter.present(&pixels, [10, 3], None),
		Ok(Presented::Shown)
	);
	assert!(!presenter.shares_memory());
	assert_eq!(shown(window), pixels);
	assert_eq!(puts(), [rows(10, 0, 2), rows(10, 2, 1)]);
	drop(presenter);
	clean();
}

#[test]
fn frames_longer_than_their_size_put_only_its_rows() {
	server(|server| server.shm = None);
	let window = window(24, [4, 4]);
	let mut presenter = presenter_with(client(), window);
	let pixels = frame(4, [4, 4]);
	assert_eq!(
		presenter.present(&pixels, [4, 2], None),
		Ok(Presented::Shown)
	);
	let expected = [&pixels[..8], &[0; 8]].concat();
	assert_eq!(shown(window), expected);
	assert_eq!(puts(), [rows(4, 0, 2)]);
	drop(presenter);
	clean();
}

thread_local! {
	/// Whether the presenter asked for MIT-SHM's version.
	static QUERIED: Cell<bool> = const { Cell::new(false) };
}

extern "C" fn query_version(_: Connection) -> Cookie {
	QUERIED.set(true);
	Cookie::default()
}

/// No version: the reply libxcb gives for a request the server refused.
extern "C" fn no_version(
	_: Connection,
	_: Cookie,
	error: *mut *mut GenericError,
) -> *mut VersionReply {
	// SAFETY: the caller passes a place for the error, which it frees; an
	// error is allocated as libxcb allocates them.
	unsafe { *error = libc::calloc(1, 32) };
	ptr::null_mut()
}

/// The fake's client, whose MIT-SHM version requests are refused.
fn versionless() -> Client {
	let mut client = client();
	client.query_version = query_version;
	client.query_version_reply = no_version;
	client
}

#[test]
fn a_server_without_mit_shm_is_not_asked_its_version() {
	// libxcb closes the connection over a request to a missing extension.
	server(|server| server.shm = None);
	let window = window(24, [2, 2]);
	let presenter = presenter_with(versionless(), window);
	assert!(!QUERIED.get());
	assert!(!presenter.shares_memory());
	drop(presenter);
	clean();
}

#[test]
fn a_version_query_the_server_refused_means_no_shared_memory() {
	let window = window(24, [2, 2]);
	let mut presenter = presenter_with(versionless(), window);
	assert!(QUERIED.get());
	assert!(!presenter.shares_memory());
	let pixels = frame(5, [2, 2]);
	assert_eq!(
		presenter.present(&pixels, [2, 2], None),
		Ok(Presented::Shown)
	);
	assert_eq!(puts(), [rows(2, 0, 2)]);
	drop(presenter);
	clean();
}
