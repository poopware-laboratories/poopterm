//! Tests of the Wayland presenter against the fake libwayland: what it
//! binds and on which queue, the buffers it makes and the rows it copies
//! and damages, how it waits for releases and follows resizes, and that it
//! leaves nothing behind, breaking no protocol rule on the way.
use super::{
	fake_tests::{self, Commit, DEFAULT_QUEUE, DISPLAY, Kind, XRGB8888, client, install, with},
	presenter::{Error, Presented, Presenter},
};

const SIZE: [u32; 2] = [8, 6];

/// A presenter on a fresh compositor whose surface is `version`.
fn presenter(version: u32) -> Presenter {
	install();
	let surface = with(|wayland| wayland.surface(version));
	// SAFETY: the fake's display and surface live as long as the thread.
	unsafe { Presenter::new(client(), DISPLAY, surface, SIZE) }.expect("a presenter")
}

/// A frame of `size` whose every pixel says where it is and which frame.
fn frame(size: [u32; 2], number: u32) -> Vec<u32> {
	let pixels = size[0] * size[1];
	(0..pixels).map(|index| number << 16 | index).collect()
}

fn last() -> Commit {
	with(|wayland| wayland.commits.last().cloned().expect("a commit"))
}

fn rules_kept() {
	let broken = with(|wayland| wayland.broken.clone());
	assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
fn binding_uses_our_own_queue_and_keeps_only_wl_shm() {
	let presenter = presenter(6);
	let (objects, queues) = with(|wayland| {
		let objects: Vec<_> = wayland
			.objects
			.values()
			.map(|o| (o.kind, o.queue, o.live, o.version))
			.collect();
		(objects, wayland.queues.clone())
	});
	let [(queue, true)] = queues.into_iter().collect::<Vec<_>>()[..] else {
		panic!("one live queue");
	};
	assert_ne!(queue, DEFAULT_QUEUE);
	// The surface, a destroyed wrapper and registry, and the bound wl_shm,
	// at version 2, which the compositor offers.
	assert_eq!(
		objects,
		[
			(Kind::Surface, DEFAULT_QUEUE, true, 6),
			(Kind::Wrapper, queue, false, 0),
			(Kind::Registry, queue, false, 0),
			(Kind::Shm, queue, true, 2),
		]
	);
	drop(presenter);
	rules_kept();
}

#[test]
fn a_compositor_without_wl_shm_or_our_format_is_refused_and_nothing_is_left() {
	for setup in [
		|wayland: &mut fake_tests::Wayland| wayland.globals.retain(|global| global.1 != c"wl_shm"),
		|wayland: &mut fake_tests::Wayland| wayland.formats.retain(|&format| format != XRGB8888),
	] {
		install();
		let surface = with(|wayland| {
			setup(wayland);
			wayland.surface(6)
		});
		// SAFETY: as in `presenter`.
		let made = unsafe { Presenter::new(client(), DISPLAY, surface, SIZE) };
		assert_eq!(made.err(), Some(Error::NoShm));
		assert_eq!(with(|wayland| wayland.leaked()), []);
		assert!(with(|wayland| wayland.queues.values().all(|live| !live)));
		rules_kept();
	}
}

#[test]
fn a_lost_connection_or_no_queue_fails_to_make_a_presenter() {
	for lose in [
		|w: &mut fake_tests::Wayland| w.lost = true,
		|w: &mut fake_tests::Wayland| w.no_queue = true,
	] {
		install();
		let surface = with(|wayland| {
			lose(wayland);
			wayland.surface(6)
		});
		// SAFETY: as in `presenter`.
		let made = unsafe { Presenter::new(client(), DISPLAY, surface, SIZE) };
		assert_eq!(made.err(), Some(Error::Lost));
		assert_eq!(with(|wayland| wayland.leaked()), []);
		rules_kept();
	}
}

#[test]
fn the_first_frame_fills_a_new_buffer_and_damages_all_of_it() {
	let mut presenter = presenter(6);
	let pixels = frame(SIZE, 1);
	assert_eq!(
		presenter.present(&pixels, SIZE, Some(&[(2, 3)])),
		Ok(Presented::Shown)
	);
	let commit = last();
	assert_eq!(commit.size, [8, 6]);
	assert_eq!(commit.pixels, pixels);
	// damage_buffer (9), as the surface is new enough.
	assert_eq!(commit.damage, [(9, [0, 0, 8, 6])]);
	let (pool, shape) = with(|wayland| wayland.objects[&commit.buffer].shape.unwrap());
	assert_eq!(shape, [0, 8, 6, 32]);
	assert_eq!(with(|wayland| wayland.objects[&pool].kind), Kind::Pool);
	assert_eq!(with(|wayland| wayland.unflushed), 0, "the frame is sent");
	drop(presenter);
	rules_kept();
}

#[test]
fn a_released_buffer_is_reused_and_only_the_changed_rows_are_damaged() {
	let mut presenter = presenter(6);
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	let first = last().buffer;
	with(|wayland| wayland.release_all());
	let mut pixels = frame(SIZE, 1);
	pixels[8..24].fill(7);
	pixels[40..48].fill(9);
	assert_eq!(
		presenter.present(&pixels, SIZE, Some(&[(5, 6), (1, 3)])),
		Ok(Presented::Shown)
	);
	let commit = last();
	assert_eq!(commit.buffer, first);
	assert_eq!(commit.pixels, pixels);
	assert_eq!(commit.damage, [(9, [0, 1, 8, 2]), (9, [0, 5, 8, 1])]);
	rules_kept();
}

#[test]
fn rows_not_copied_into_a_reused_buffer_keep_what_it_showed() {
	// The copy really is partial: a row the caller wrongly says is
	// unchanged keeps the buffer's old pixels.
	let mut presenter = presenter(6);
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	with(|wayland| wayland.release_all());
	presenter
		.present(&frame(SIZE, 2), SIZE, Some(&[(0, 1)]))
		.unwrap();
	let (new, old) = (frame(SIZE, 2), frame(SIZE, 1));
	assert_eq!(last().pixels[..8], new[..8]);
	assert_eq!(last().pixels[8..], old[8..]);
}

#[test]
fn surfaces_before_version_4_get_surface_damage_and_later_ones_buffer_damage() {
	for (version, opcode) in [(1, 2), (3, 2), (4, 9)] {
		let mut presenter = presenter(version);
		presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
		assert_eq!(last().damage, [(opcode, [0, 0, 8, 6])], "version {version}");
		drop(presenter);
		rules_kept();
	}
}

#[test]
fn held_buffers_make_new_ones_up_to_three_then_frames_wait_and_carry_over() {
	let mut presenter = presenter(6);
	for number in 1..=3 {
		assert_eq!(
			presenter.present(&frame(SIZE, number), SIZE, None),
			Ok(Presented::Shown)
		);
	}
	let buffers = with(|wayland| wayland.held());
	assert_eq!(buffers.len(), 3);
	let commits = with(|wayland| wayland.commits.len());
	let mut pixels = frame(SIZE, 3);
	pixels[0..8].fill(5);
	assert_eq!(
		presenter.present(&pixels, SIZE, Some(&[(0, 1)])),
		Ok(Presented::Busy)
	);
	assert_eq!(with(|wayland| wayland.commits.len()), commits);
	// The oldest is released; the next frame shows what changed meanwhile.
	with(|wayland| wayland.release(buffers[0]));
	pixels[40..48].fill(6);
	assert_eq!(
		presenter.present(&pixels, SIZE, Some(&[(5, 6)])),
		Ok(Presented::Shown)
	);
	let commit = last();
	assert_eq!(commit.buffer, buffers[0]);
	assert_eq!(commit.pixels, pixels);
	assert_eq!(commit.damage, [(9, [0, 0, 8, 1]), (9, [0, 5, 8, 1])]);
	drop(presenter);
	rules_kept();
}

#[test]
fn resizing_destroys_free_buffers_now_and_held_ones_once_released() {
	let mut presenter = presenter(6);
	presenter.present(&frame(SIZE, 1), SIZE, None).unwrap();
	presenter.present(&frame(SIZE, 2), SIZE, None).unwrap();
	let [free, held] = with(|wayland| wayland.held())[..] else {
		panic!("two buffers")
	};
	// Both released; the next frame learns it and shows in the newer one,
	// leaving the other known to be free.
	with(|wayland| wayland.release_all());
	presenter.present(&frame(SIZE, 2), SIZE, Some(&[])).unwrap();
	assert_eq!(last().buffer, held);
	let size = [5, 4];
	let pixels = frame(size, 3);
	assert_eq!(
		presenter.present(&pixels, size, Some(&[])),
		Ok(Presented::Shown)
	);
	assert_eq!(last().pixels, pixels);
	assert_eq!(last().damage, [(9, [0, 0, 5, 4])]);
	let live = |buffer| with(|wayland| wayland.objects[&buffer].live);
	assert!(!live(free));
	assert!(live(held));
	with(|wayland| wayland.release(held));
	presenter.present(&pixels, size, Some(&[])).unwrap();
	assert!(!live(held));
	drop(presenter);
	rules_kept();
}

#[test]
fn a_window_without_area_shows_nothing_and_sends_nothing() {
	let mut presenter = presenter(6);
	let requests = with(|wayland| wayland.requests);
	assert_eq!(presenter.present(&[], [0, 6], None), Ok(Presented::Busy));
	assert_eq!(presenter.present(&[], [8, 0], None), Ok(Presented::Busy));
	assert_eq!(with(|wayland| wayland.requests), requests);
	// The next frame with area is whole.
	presenter.present(&frame(SIZE, 1), SIZE, Some(&[])).unwrap();
	assert_eq!(last().damage, [(9, [0, 0, 8, 6])]);
	rules_kept();
}

#[test]
fn a_frame_too_large_for_the_protocol_fails_and_the_next_one_still_shows() {
	let mut presenter = presenter(6);
	let huge = [32768, 16384];
	let requests = with(|wayland| wayland.requests);
	let failed = presenter.present(&[], huge, None);
	assert!(matches!(failed, Err(Error::Memory(_))), "{failed:?}");
	assert_eq!(with(|wayland| wayland.requests), requests);
	// Trying again fails the same way; no buffer is left half made.
	let failed = presenter.present(&[], huge, None);
	assert!(matches!(failed, Err(Error::Memory(_))), "{failed:?}");
	assert_eq!(with(|wayland| wayland.requests), requests);
	assert_eq!(
		presenter.present(&frame(SIZE, 1), SIZE, None),
		Ok(Presented::Shown)
	);
	assert_eq!(last().pixels, frame(SIZE, 1));
	drop(presenter);
	rules_kept();
}

#[test]
fn a_lost_connection_fails_the_frame() {
	let mut presenter = presenter(6);
	with(|wayland| wayland.lost = true);
	assert_eq!(
		presenter.present(&frame(SIZE, 1), SIZE, None),
		Err(Error::Lost)
	);
}

#[test]
fn dropping_destroys_every_buffer_pool_and_wl_shm_and_then_the_queue() {
	for (offered, kept) in [(3, 2), (2, 2), (1, 1)] {
		install();
		let surface = with(|wayland| {
			wayland
				.globals
				.iter_mut()
				.for_each(|global| global.2 = offered);
			wayland.surface(6)
		});
		// SAFETY: as in `presenter`.
		let mut presenter = unsafe { Presenter::new(client(), DISPLAY, surface, SIZE) }.unwrap();
		assert_eq!(
			with(|wayland| wayland
				.objects
				.values()
				.find(|o| o.kind == Kind::Shm)
				.unwrap()
				.version),
			kept
		);
		for number in 1..=3 {
			presenter.present(&frame(SIZE, number), SIZE, None).unwrap();
		}
		drop(presenter);
		assert_eq!(with(|wayland| wayland.leaked()), []);
		assert!(with(|wayland| wayland.queues.values().all(|live| !live)));
		rules_kept();
	}
}

#[test]
fn every_frame_shows_whole_with_its_changes_damaged_whatever_the_compositor_does() {
	// A random run of frames, releases and resizes: each commit must equal
	// the frame drawn, every row that differs from the last commit of the
	// same buffer size must be damaged, and no rule may break.
	let mut rng = crate::bench::corpus::Rng::new(0x3a71_0bed);
	let mut presenter = presenter(6);
	let mut size = SIZE;
	let mut drawn = frame(size, 0);
	let mut changed: Option<Vec<(usize, usize)>> = None;
	let mut previous: Option<Commit> = None;
	for number in 1..1500u32 {
		match rng.below(10) {
			0..=2 => {
				let held = with(|wayland| wayland.held());
				if !held.is_empty() {
					with(|wayland| wayland.release(held[rng.below(held.len())]));
				}
			}
			3 => {
				size = [1 + rng.below(9) as u32, 1 + rng.below(12) as u32];
				drawn = frame(size, number);
				changed = None;
			}
			_ => {
				let [width, height] = size.map(|side| side as usize);
				let start = rng.below(height + 1);
				let band = (start, (start + rng.below(4)).min(height));
				drawn[band.0 * width..band.1 * width].fill(number);
				if let Some(changed) = &mut changed {
					changed.push(band);
				}
				let commits = with(|wayland| wayland.commits.len());
				match presenter.present(&drawn, size, changed.as_deref()).unwrap() {
					Presented::Busy => assert_eq!(with(|wayland| wayland.commits.len()), commits),
					Presented::Shown => {
						let commit = last();
						assert_eq!(commit.pixels, drawn, "frame {number}");
						assert_eq!(with(|wayland| wayland.unflushed), 0, "frame {number} sent");
						let damaged = |row: i32| {
							commit
								.damage
								.iter()
								.any(|&(_, [_, y, _, h])| (y..y + h).contains(&row))
						};
						match previous
							.as_ref()
							.filter(|previous| previous.size == commit.size)
						{
							Some(previous) => {
								let rows = previous
									.pixels
									.chunks(width)
									.zip(commit.pixels.chunks(width));
								for (row, (old, new)) in rows.enumerate() {
									assert!(
										old == new || damaged(row as i32),
										"row {row} of frame {number}"
									);
								}
							}
							None => assert!((0..height as i32).all(damaged)),
						}
						previous = Some(commit);
						changed = Some(Vec::new());
					}
				}
			}
		}
	}
	drop(presenter);
	assert_eq!(with(|wayland| wayland.leaked()), []);
	rules_kept();
}
