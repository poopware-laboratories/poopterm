//! Tests of the window buffer pool: which buffer each frame is drawn into,
//! how old its contents are, and when buffers are made and destroyed as
//! the compositor holds and releases them and the window changes size.
use super::pool::{MAX_BUFFERS, Pick, Pool};

const SIZE: [u32; 2] = [640, 480];

/// Picks a buffer and presents it, making one if the pool says to: its id
/// and age.
fn show(pool: &mut Pool) -> (u64, u8) {
	let (id, age) = match pool.next() {
		Pick::Reuse { id, age } => (id, age),
		Pick::Make { id } => (id, 0),
		Pick::Wait => panic!("every buffer is held"),
	};
	pool.presented(id);
	(id, age)
}

#[test]
fn the_first_frame_makes_a_buffer_of_the_windows_size() {
	let mut pool = Pool::new(SIZE);
	assert_eq!(pool.next(), Pick::Make { id: 1 });
	assert_eq!(pool.size(), SIZE);
}

#[test]
fn a_buffer_released_before_the_next_frame_is_reused_holding_that_frame() {
	let mut pool = Pool::new(SIZE);
	assert_eq!(show(&mut pool), (1, 0));
	assert!(!pool.released(1));
	// Age 1: it holds the frame just shown, so only new damage is copied.
	assert_eq!(show(&mut pool), (1, 1));
}

#[test]
fn a_buffer_still_held_makes_another_and_ages_count_frames_since_shown() {
	let mut pool = Pool::new(SIZE);
	assert_eq!(show(&mut pool), (1, 0));
	assert_eq!(show(&mut pool), (2, 0));
	pool.released(1);
	// Buffer 1 shows the frame before last.
	assert_eq!(show(&mut pool), (1, 2));
	pool.released(2);
	assert_eq!(show(&mut pool), (2, 2));
	pool.released(1);
	pool.released(2);
	// With both free, the one holding the newer frame wins.
	assert_eq!(show(&mut pool), (2, 1));
}

#[test]
fn no_more_than_the_most_buffers_are_made_then_the_frame_waits() {
	let mut pool = Pool::new(SIZE);
	let ids: Vec<_> = (0..MAX_BUFFERS).map(|_| show(&mut pool).0).collect();
	assert_eq!(ids, [1, 2, 3]);
	assert_eq!(pool.next(), Pick::Wait);
	pool.released(2);
	assert_eq!(pool.next(), Pick::Reuse { id: 2, age: 2 });
}

#[test]
fn a_buffer_made_but_not_presented_is_free_and_holds_nothing() {
	let mut pool = Pool::new(SIZE);
	assert_eq!(pool.next(), Pick::Make { id: 1 });
	// A frame not drawn after all: the buffer is made, and offered again.
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 0 });
	show(&mut pool);
	pool.released(1);
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 1 });
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 1 });
}

#[test]
fn resizing_destroys_free_buffers_at_once_and_held_ones_once_released() {
	let mut pool = Pool::new(SIZE);
	show(&mut pool);
	show(&mut pool);
	pool.released(1);
	assert_eq!(pool.resize([800, 600]), [1]);
	assert_eq!(pool.size(), [800, 600]);
	// The held one is not reused, and goes when the compositor lets go.
	assert_eq!(pool.next(), Pick::Make { id: 3 });
	assert!(pool.released(2));
	assert_eq!(show(&mut pool), (3, 0));
}

#[test]
fn resizing_to_the_same_size_keeps_every_buffer() {
	let mut pool = Pool::new(SIZE);
	show(&mut pool);
	pool.released(1);
	assert_eq!(pool.resize(SIZE), Vec::<u64>::new());
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 1 });
}

#[test]
fn buffers_left_held_from_an_old_size_do_not_count_toward_the_most() {
	let mut pool = Pool::new(SIZE);
	for _ in 0..MAX_BUFFERS {
		show(&mut pool);
	}
	pool.resize([800, 600]);
	// The compositor may hold old buffers a while; the window still draws.
	assert_eq!(show(&mut pool), (4, 0));
	assert_eq!(pool.live(), 4);
	for id in 1..=3 {
		assert!(pool.released(id));
	}
	assert_eq!(pool.live(), 1);
}

#[test]
fn releases_of_unknown_or_free_buffers_change_nothing() {
	let mut pool = Pool::new(SIZE);
	show(&mut pool);
	assert!(!pool.released(9));
	assert!(!pool.released(1));
	assert!(!pool.released(1));
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 1 });
}

#[test]
fn ages_past_what_a_byte_holds_read_as_unknown() {
	let mut pool = Pool::new(SIZE);
	show(&mut pool);
	show(&mut pool);
	pool.released(1);
	// Buffer 2 shows frame after frame while buffer 1 sits free.
	for _ in 0..300 {
		pool.released(2);
		assert_eq!(show(&mut pool), (2, 1));
	}
	// With buffer 2 held, buffer 1 is too old for its age to say anything.
	assert_eq!(pool.next(), Pick::Reuse { id: 1, age: 0 });
}

#[test]
fn a_zero_sized_window_makes_no_buffers() {
	let mut pool = Pool::new([0, 480]);
	assert_eq!(pool.next(), Pick::Wait);
	assert_eq!(pool.resize([640, 0]), Vec::<u64>::new());
	assert_eq!(pool.next(), Pick::Wait);
	pool.resize(SIZE);
	assert_eq!(pool.next(), Pick::Make { id: 1 });
}

#[test]
fn every_frame_shown_is_whole_whatever_the_compositor_holds_and_however_the_window_resizes() {
	// A random run of frames, releases and resizes, with each buffer's rows
	// copied as `History` plans for the age the pool gives: every frame
	// shown must equal the one drawn, no held buffer is ever drawn into,
	// and every buffer made is destroyed once. Rows changed while every
	// buffer is held carry over to the next frame shown.
	use super::copy::{History, Rows};
	use std::collections::HashMap;
	let mut rng = crate::bench::corpus::Rng::new(0xb0ff_e125);
	let mut pool = Pool::new([4, 16]);
	let mut history = History::new();
	let mut buffers: HashMap<u64, Vec<u32>> = HashMap::new();
	let mut held: Vec<u64> = Vec::new();
	let (mut destroyed, mut made) = (Vec::new(), 0);
	let mut drawn = vec![0u32; 16];
	for frame in 1..2000u32 {
		match rng.below(10) {
			// The compositor lets go of a buffer it holds.
			0..=2 if !held.is_empty() => {
				let id = held.swap_remove(rng.below(held.len()));
				if pool.released(id) {
					destroyed.push(id);
					buffers.remove(&id);
				}
			}
			// The window changes height, which redraws it whole.
			3 => {
				let rows = 1 + rng.below(20);
				let gone = pool.resize([4, rows as u32]);
				if drawn.len() != rows {
					history.clear();
				}
				history.changed(None);
				for id in gone {
					assert!(!held.contains(&id), "{id} destroyed while held");
					buffers.remove(&id);
					destroyed.push(id);
				}
				drawn = vec![frame; rows];
			}
			_ => {
				let rows = drawn.len();
				let start = rng.below(rows + 1);
				let band = (start, (start + rng.below(4)).min(rows));
				drawn[band.0..band.1].fill(frame);
				history.changed(Some(&[band]));
				let (id, age) = match pool.next() {
					Pick::Wait => continue,
					Pick::Make { id } => {
						made += 1;
						assert_eq!(id, made, "ids are never reused");
						buffers.insert(id, vec![u32::MAX; rows]);
						(id, 0)
					}
					Pick::Reuse { id, age } => (id, age),
				};
				assert!(!held.contains(&id), "{id} drawn into while held");
				let buffer = buffers.get_mut(&id).unwrap();
				assert_eq!(buffer.len(), rows, "a buffer of the window's size");
				match history.copy(age, rows) {
					Rows::All => buffer.copy_from_slice(&drawn),
					Rows::Some(copied) => {
						for (start, end) in copied {
							buffer[start..end].copy_from_slice(&drawn[start..end]);
						}
					}
				}
				assert_eq!(*buffer, drawn, "frame {frame} into {id} at age {age}");
				pool.presented(id);
				history.shown();
				held.push(id);
				let current = buffers.keys().filter(|id| !destroyed.contains(id)).count();
				assert!(current <= pool.live());
			}
		}
	}
	for id in held {
		if pool.released(id) {
			destroyed.push(id);
		}
	}
	let mut destroyed_sorted = destroyed.clone();
	destroyed_sorted.sort_unstable();
	destroyed_sorted.dedup();
	assert_eq!(
		destroyed_sorted.len(),
		destroyed.len(),
		"each destroyed once"
	);
	assert_eq!(destroyed.len() + pool.live(), made as usize);
}

#[test]
fn a_buffer_that_could_not_be_made_is_forgotten_and_its_id_never_reused() {
	let mut pool = Pool::new(SIZE);
	assert_eq!(pool.next(), Pick::Make { id: 1 });
	pool.forget(1);
	assert_eq!(pool.live(), 0);
	// Not offered as a free buffer that holds nothing: it does not exist.
	assert_eq!(pool.next(), Pick::Make { id: 2 });
	show(&mut pool);
	// Forgetting a buffer that is shown or unknown changes nothing else.
	pool.forget(9);
	assert_eq!(pool.live(), 1);
	assert!(!pool.released(2));
	assert_eq!(pool.next(), Pick::Reuse { id: 2, age: 1 });
}
