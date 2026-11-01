//! Tests of the X11 presenter against the fake server: frames reach the
//! window whole, through shared memory or `PutImage` requests, buffers are
//! reused only once the server has read them, and what it is given is
//! taken back.
use super::{
	fake_tests::{DISPLAY, Put, client, server, setup, window},
	presenter::{Error, Presented, Presenter},
};
use crate::present::pool::MAX_BUFFERS;

/// A frame whose every pixel differs from those of other seeds.
fn frame(seed: u32, [width, height]: [usize; 2]) -> Vec<u32> {
	(0..width * height)
		.map(|index| seed << 24 | index as u32)
		.collect()
}

fn presenter(window: u32) -> Presenter {
	// SAFETY: the fake display is live for the thread.
	unsafe { Presenter::new(client(), DISPLAY, window) }.expect("a presenter")
}

/// What `window` shows once the server served every request.
fn shown(window: u32) -> Vec<u32> {
	server(|server| {
		server.serve();
		server.windows[&window].pixels.clone()
	})
}

/// Requires that no rule was broken and, after every request is served,
/// that nothing given to the server is left.
fn clean() {
	server(|server| {
		server.serve();
		assert_eq!(server.broken, Vec::<String>::new());
		assert!(server.gcs.is_empty(), "GCs left: {:?}", server.gcs);
		assert!(
			server.segments.is_empty(),
			"segments left: {}",
			server.segments.len()
		);
		assert!(
			server.replies.is_empty(),
			"replies left: {:?}",
			server.replies
		);
	});
}

fn puts() -> Vec<Put> {
	server(|server| {
		server.serve();
		std::mem::take(&mut server.puts)
	})
}

#[test]
fn frames_go_through_shared_memory_and_show_whole() {
	let window = window(24, [8, 5]);
	let mut presenter = presenter(window);
	assert!(presenter.shares_memory());
	for seed in 1..=4 {
		let pixels = frame(seed, [8, 5]);
		assert_eq!(
			presenter.present(&pixels, [8, 5], None),
			Ok(Presented::Shown)
		);
		assert_eq!(shown(window), pixels, "frame {seed}");
	}
	let shared = Put {
		shared: true,
		size: [8, 5],
		y: 0,
	};
	assert_eq!(puts(), vec![shared; 4]);
	assert_eq!(server(|server| server.flushes), 4);
	drop(presenter);
	clean();
}

#[test]
fn servers_without_memory_by_descriptor_get_put_image_requests() {
	for shm in [None, Some((1, 1)), Some((0, 9))] {
		server(|server| server.shm = shm);
		let window = window(24, [6, 4]);
		let mut presenter = presenter(window);
		assert!(!presenter.shares_memory(), "{shm:?}");
		let pixels = frame(7, [6, 4]);
		assert_eq!(
			presenter.present(&pixels, [6, 4], None),
			Ok(Presented::Shown)
		);
		assert_eq!(shown(window), pixels);
		assert_eq!(
			puts(),
			vec![Put {
				shared: false,
				size: [6, 4],
				y: 0
			}]
		);
		drop(presenter);
		clean();
	}
}

#[test]
fn later_versions_of_mit_shm_share_memory_too() {
	server(|server| server.shm = Some((2, 0)));
	assert!(presenter(window(24, [1, 1])).shares_memory());
	clean();
}

#[test]
fn a_server_that_cannot_map_our_memory_gets_put_image_requests_from_then_on() {
	server(|server| server.refuse_attach = true);
	let window = window(24, [5, 3]);
	let mut presenter = presenter(window);
	for seed in 1..=2 {
		let pixels = frame(seed, [5, 3]);
		assert_eq!(
			presenter.present(&pixels, [5, 3], None),
			Ok(Presented::Shown)
		);
		assert_eq!(shown(window), pixels);
		assert!(!presenter.shares_memory());
	}
	assert_eq!(
		puts(),
		vec![
			Put {
				shared: false,
				size: [5, 3],
				y: 0
			};
			2
		]
	);
	drop(presenter);
	clean();
}

#[test]
fn memory_attached_before_the_server_refused_more_is_detached() {
	let window = window(24, [3, 2]);
	let mut presenter = presenter(window);
	assert_eq!(
		presenter.present(&frame(1, [3, 2]), [3, 2], None),
		Ok(Presented::Shown)
	);
	server(|server| server.refuse_attach = true);
	let pixels = frame(2, [3, 2]);
	assert_eq!(
		presenter.present(&pixels, [3, 2], None),
		Ok(Presented::Shown)
	);
	assert!(!presenter.shares_memory());
	assert_eq!(shown(window), pixels);
	assert_eq!(server(|server| server.segments.len()), 0);
	drop(presenter);
	clean();
}

#[test]
fn put_image_requests_hold_as_many_whole_rows_as_the_server_takes() {
	server(|server| {
		server.shm = None;
		// Six units of header and three rows of ten pixels.
		server.max_units = 6 + 30;
	});
	let window = window(24, [10, 7]);
	let mut presenter = presenter(window);
	let pixels = frame(3, [10, 7]);
	assert_eq!(
		presenter.present(&pixels, [10, 7], Some(&[(2, 3)])),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), pixels);
	let rows = |y, height| Put {
		shared: false,
		size: [10, height],
		y,
	};
	assert_eq!(puts(), vec![rows(0, 3), rows(3, 3), rows(6, 1)]);
	drop(presenter);
	clean();
}

#[test]
fn rows_wider_than_any_request_are_an_error() {
	server(|server| {
		server.shm = None;
		server.max_units = 6 + 9;
	});
	let window = window(24, [10, 2]);
	let mut presenter = presenter(window);
	let error = Error::Memory("rows of 10 pixels".to_owned());
	assert_eq!(
		presenter.present(&frame(1, [10, 2]), [10, 2], None),
		Err(error)
	);
	assert_eq!(puts(), []);
	let narrow = frame(2, [9, 2]);
	assert_eq!(
		presenter.present(&narrow, [9, 2], None),
		Ok(Presented::Shown)
	);
	drop(presenter);
	clean();
}

#[test]
fn buffers_the_server_has_not_read_are_left_alone() {
	let window = window(24, [4, 4]);
	let mut presenter = presenter(window);
	// Attaching a buffer waits for the server, which then reads every frame
	// before: it takes a few frames for all of them to be held.
	let mut shown_last = None;
	for seed in 1..=8 {
		let pixels = frame(seed, [4, 4]);
		match presenter.present(&pixels, [4, 4], None) {
			Ok(Presented::Shown) => shown_last = Some(pixels),
			busy => {
				assert_eq!(busy, Ok(Presented::Busy));
				break;
			}
		}
	}
	assert_eq!(server(|server| server.segments.len()), MAX_BUFFERS);
	assert_eq!(Some(shown(window)), shown_last);
	let pixels = frame(9, [4, 4]);
	assert_eq!(
		presenter.present(&pixels, [4, 4], None),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), pixels);
	assert_eq!(server(|server| server.segments.len()), MAX_BUFFERS);
	drop(presenter);
	clean();
}

#[test]
fn rows_outside_the_bands_come_from_older_frames_in_each_buffer() {
	let size = [3, 6];
	let window = window(24, size);
	let mut presenter = presenter(window);
	let mut pixels = frame(1, size);
	assert_eq!(
		presenter.present(&pixels, [3, 6], None),
		Ok(Presented::Shown)
	);
	// Each frame changes one band; buffers of every age are reused, some
	// before the server read the last frame.
	let bands: [&[(usize, usize)]; 6] = [
		&[(0, 1)],
		&[(5, 6)],
		&[(2, 4)],
		&[],
		&[(1, 2), (4, 5)],
		&[(3, 4)],
	];
	for (step, rows) in bands.into_iter().enumerate() {
		for &(start, end) in rows {
			for pixel in &mut pixels[start * 3..end * 3] {
				*pixel ^= (step as u32 + 2) << 24;
			}
		}
		assert_eq!(
			presenter.present(&pixels, [3, 6], Some(rows)),
			Ok(Presented::Shown)
		);
		if step % 2 == 1 {
			assert_eq!(shown(window), pixels, "step {step}");
		}
	}
	assert_eq!(shown(window), pixels);
	drop(presenter);
	clean();
}

#[test]
fn a_resize_makes_buffers_of_the_new_size_and_detaches_the_old_ones() {
	let window = window(24, [6, 6]);
	let mut presenter = presenter(window);
	assert_eq!(
		presenter.present(&frame(1, [6, 6]), [6, 6], None),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), frame(1, [6, 6]));
	// The old buffer is still held when the size changes, and bands are of
	// the old frame: the whole new one is copied.
	let small = frame(2, [4, 3]);
	assert_eq!(
		presenter.present(&small, [4, 3], Some(&[(0, 1)])),
		Ok(Presented::Shown)
	);
	assert_eq!(
		puts().last(),
		Some(&Put {
			shared: true,
			size: [4, 3],
			y: 0
		})
	);
	assert_eq!(&shown(window)[..4], &small[..4]);
	assert_eq!(&shown(window)[6..10], &small[4..8]);
	assert_eq!(
		presenter.present(&frame(3, [4, 3]), [4, 3], None),
		Ok(Presented::Shown)
	);
	assert_eq!(
		server(|server| server.segments.len()),
		1,
		"the old one detached once read"
	);
	let back = frame(4, [6, 6]);
	assert_eq!(
		presenter.present(&back, [6, 6], Some(&[(5, 6)])),
		Ok(Presented::Shown)
	);
	assert_eq!(shown(window), back);
	drop(presenter);
	clean();
}

#[test]
fn free_buffers_are_detached_when_the_size_changes() {
	let window = window(24, [5, 5]);
	let mut presenter = presenter(window);
	let mut seed = 0;
	while server(|server| server.segments.len()) < MAX_BUFFERS {
		seed += 1;
		assert_eq!(
			presenter.present(&frame(seed, [5, 5]), [5, 5], None),
			Ok(Presented::Shown)
		);
	}
	server(|server| server.serve());
	// One buffer shows this frame; the others are free.
	assert_eq!(
		presenter.present(&frame(9, [5, 5]), [5, 5], None),
		Ok(Presented::Shown)
	);
	assert_eq!(
		presenter.present(&frame(10, [5, 4]), [5, 4], None),
		Ok(Presented::Shown)
	);
	assert_eq!(
		server(|server| server.segments.len()),
		2,
		"the one shown, and the new one"
	);
	drop(presenter);
	clean();
}

#[test]
fn only_the_rows_in_the_bands_are_copied_into_a_buffer() {
	let window = window(24, [2, 3]);
	let mut presenter = presenter(window);
	let old = frame(1, [2, 3]);
	assert_eq!(presenter.present(&old, [2, 3], None), Ok(Presented::Shown));
	server(|server| server.serve());
	// Every row differs, but the frame says only the middle one changed:
	// the buffer, which holds the old frame, gets that one alone.
	let new = frame(2, [2, 3]);
	assert_eq!(
		presenter.present(&new, [2, 3], Some(&[(1, 2)])),
		Ok(Presented::Shown)
	);
	let expected = [&old[..2], &new[2..4], &old[4..]].concat();
	assert_eq!(shown(window), expected);
	drop(presenter);
	clean();
}

#[test]
fn frames_without_area_send_nothing() {
	let window = window(24, [2, 2]);
	let mut presenter = presenter(window);
	for size in [[0, 0], [0, 5], [5, 0]] {
		assert_eq!(presenter.present(&[], size, None), Ok(Presented::Shown));
	}
	assert_eq!(puts(), []);
	assert_eq!(server(|server| server.segments.len()), 0);
	drop(presenter);
	clean();
}

#[test]
fn frames_larger_than_x11_allows_are_an_error() {
	let window = window(24, [2, 2]);
	let mut presenter = presenter(window);
	let error = |size: &str| Err(Error::Memory(format!("a frame of {size} pixels")));
	assert_eq!(presenter.present(&[], [65536, 1], None), error("65536×1"));
	assert_eq!(presenter.present(&[], [1, 65536], None), error("1×65536"));
	let pixels = vec![9; 65535];
	assert_eq!(
		presenter.present(&pixels, [65535, 1], None),
		Ok(Presented::Shown)
	);
	assert_eq!(&shown(window)[..2], &[9, 9]);
	drop(presenter);
	clean();
}

#[test]
fn frames_too_large_to_share_are_an_error_and_leave_no_buffer_behind() {
	let window = window(24, [4, 2]);
	let mut presenter = presenter(window);
	// More than the 2 GiB a buffer may take.
	for _ in 0..2 {
		let presented = presenter.present(&[], [65535, 8193], None);
		let message = format!("{presented:?}");
		assert!(
			message.starts_with("Err(Memory(\"65535×8193 pixels:"),
			"{message}"
		);
	}
	for seed in 1..=4 {
		let pixels = frame(seed, [4, 2]);
		assert_eq!(
			presenter.present(&pixels, [4, 2], None),
			Ok(Presented::Shown)
		);
		assert_eq!(shown(window), pixels);
	}
	drop(presenter);
	clean();
}

#[test]
fn frames_shorter_than_their_size_show_the_rows_they_have() {
	for shm in [Some((1, 2)), None] {
		server(|server| server.shm = shm);
		let window = window(24, [4, 3]);
		let mut presenter = presenter(window);
		let pixels = frame(5, [4, 3]);
		assert_eq!(
			presenter.present(&pixels[..9], [4, 3], None),
			Ok(Presented::Shown)
		);
		let mut expected = pixels[..8].to_vec();
		if shm.is_some() {
			// The rest of a fresh buffer, zeroed.
			expected.extend([pixels[8], 0, 0, 0]);
		} else {
			expected.extend([0; 4]);
		}
		assert_eq!(shown(window), expected, "{shm:?}");
		drop(presenter);
		clean();
	}
}

#[test]
fn a_lost_connection_is_reported() {
	let window = window(24, [2, 2]);
	let mut presenter = presenter(window);
	server(|server| server.lost = true);
	assert_eq!(
		presenter.present(&frame(1, [2, 2]), [2, 2], None),
		Err(Error::Lost)
	);
	drop(presenter);
	// SAFETY: the fake display is live for the thread.
	let again = unsafe { Presenter::new(client(), DISPLAY, window) };
	assert_eq!(again.err(), Some(Error::Lost));
}

#[test]
fn a_display_without_a_connection_gets_no_presenter() {
	let window = window(24, [2, 2]);
	// SAFETY: the fake treats any display but its own as unconnected.
	let presenter = unsafe { Presenter::new(client(), std::ptr::null_mut(), window) };
	assert_eq!(presenter.err(), Some(Error::Lost));
	clean();
}

#[test]
fn windows_that_do_not_take_our_pixels_get_no_presenter() {
	let shallow = window(16, [2, 2]);
	// SAFETY: the fake display is live for the thread.
	let presenter = unsafe { Presenter::new(client(), DISPLAY, shallow) };
	assert_eq!(presenter.err(), Some(Error::Format));
	let deep = window(24, [2, 2]);
	server(|server| server.setup = setup(&[(24, 24)]));
	// SAFETY: as above.
	let presenter = unsafe { Presenter::new(client(), DISPLAY, deep) };
	assert_eq!(presenter.err(), Some(Error::Format));
	server(|server| server.setup[30] ^= 1);
	// SAFETY: as above.
	let presenter = unsafe { Presenter::new(client(), DISPLAY, deep) };
	assert_eq!(presenter.err(), Some(Error::Format));
	clean();
}

#[test]
fn a_refused_gc_or_unknown_window_gets_no_presenter() {
	let window = window(24, [2, 2]);
	server(|server| server.refuse_gc = true);
	// SAFETY: the fake display is live for the thread.
	let presenter = unsafe { Presenter::new(client(), DISPLAY, window) };
	assert_eq!(presenter.err(), Some(Error::Lost));
	server(|server| {
		server.refuse_gc = false;
		server.windows.clear();
	});
	// SAFETY: as above.
	let presenter = unsafe { Presenter::new(client(), DISPLAY, window) };
	assert_eq!(presenter.err(), Some(Error::Lost));
	server(|server| {
		server
			.broken
			.retain(|rule| rule != "the geometry of no window")
	});
	clean();
}

#[test]
fn dropping_while_the_server_reads_takes_everything_back() {
	let window = window(24, [3, 3]);
	let mut presenter = presenter(window);
	for seed in 1..=2 {
		assert_eq!(
			presenter.present(&frame(seed, [3, 3]), [3, 3], None),
			Ok(Presented::Shown)
		);
	}
	server(|server| server.serve());
	assert_eq!(
		presenter.present(&frame(3, [3, 3]), [3, 3], None),
		Ok(Presented::Shown)
	);
	drop(presenter);
	assert_eq!(shown(window), frame(3, [3, 3]));
	clean();
}
