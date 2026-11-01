//! Tests of the X11 presenter's requests at their bounds, against the fake
//! server: `PutImage` requests as long as the server takes and no longer,
//! and the requests that take everything back sent off when it drops.
use super::{
	fake_tests::{DISPLAY, Put, client, server, window},
	presenter::{Presented, Presenter},
};

fn presenter(window: u32) -> Presenter {
	// SAFETY: the fake display is live for the thread.
	unsafe { Presenter::new(client(), DISPLAY, window) }.expect("a presenter")
}

#[test]
fn put_image_requests_reach_the_servers_maximum_but_never_pass_it() {
	server(|server| {
		server.shm = None;
		// Six units of header and three rows of one pixel: one byte more
		// than that would still not fit a fourth row, four would.
		server.max_units = 6 + 3;
	});
	let window = window(24, [1, 7]);
	let mut presenter = presenter(window);
	let pixels: Vec<u32> = (1..=7).collect();
	assert_eq!(
		presenter.present(&pixels, [1, 7], None),
		Ok(Presented::Shown)
	);
	let rows = |y, height| Put {
		shared: false,
		size: [1, height],
		y,
	};
	server(|server| {
		server.serve();
		assert_eq!(server.broken, Vec::<String>::new());
		assert_eq!(server.puts, [rows(0, 3), rows(3, 3), rows(6, 1)]);
		assert_eq!(server.windows[&window].pixels, pixels);
	});
}

#[test]
fn dropping_flushes_the_requests_that_take_everything_back() {
	let window = window(24, [2, 2]);
	let mut presenter = presenter(window);
	assert_eq!(
		presenter.present(&[1, 2, 3, 4], [2, 2], None),
		Ok(Presented::Shown)
	);
	let flushes = server(|server| server.flushes);
	drop(presenter);
	// Unflushed, the detach and the GC's release would wait in libxcb's
	// buffer for whatever request the display sends next.
	assert_eq!(server(|server| server.flushes), flushes + 1);
}
