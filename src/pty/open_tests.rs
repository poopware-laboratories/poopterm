//! Tests for opening PTYs one at a time.

use super::*;
use std::{
	fs::File,
	io::{Read, Write},
	panic,
	sync::{
		atomic::{AtomicUsize, Ordering::SeqCst},
		mpsc,
	},
	thread,
	time::Duration,
};

const SIZE: PtySize = PtySize {
	rows: 24,
	cols: 80,
	pixel_width: 0,
	pixel_height: 0,
};

#[test]
fn openings_never_overlap() {
	let (inside, most) = (AtomicUsize::new(0), AtomicUsize::new(0));
	thread::scope(|scope| {
		for _ in 0..8 {
			scope.spawn(|| {
				for _ in 0..20 {
					one_at_a_time(|| {
						most.fetch_max(inside.fetch_add(1, SeqCst) + 1, SeqCst);
						thread::sleep(Duration::from_micros(200));
						inside.fetch_sub(1, SeqCst);
					});
				}
			});
		}
	});
	assert_eq!(most.into_inner(), 1);
}

#[test]
fn what_the_opener_returns_is_returned() {
	assert_eq!(one_at_a_time(|| 42), 42);
}

#[test]
fn a_panic_while_opening_does_not_stop_later_openings() {
	let caught = panic::catch_unwind(|| one_at_a_time(|| panic!("no PTY")));
	assert!(caught.is_err());
	assert_eq!(one_at_a_time(|| "again"), "again");
}

/// Writes `token` to the slave by its path and returns what the master
/// heard, or less if it heard nothing for ten seconds.
fn heard_through(pair: &PtyPair, token: &[u8]) -> Vec<u8> {
	let path = pair.master.tty_name().expect("the slave has a path");
	let mut reader = pair.master.try_clone_reader().unwrap();
	let (send, heard) = mpsc::channel();
	let want = token.len();
	thread::spawn(move || {
		let (mut got, mut buf) = (Vec::new(), [0; 64]);
		while got.len() < want {
			match reader.read(&mut buf) {
				Ok(n @ 1..) => got.extend_from_slice(&buf[..n]),
				_ => break,
			}
		}
		drop(send.send(got));
	});
	File::options()
		.write(true)
		.open(path)
		.unwrap()
		.write_all(token)
		.unwrap();
	heard
		.recv_timeout(Duration::from_secs(10))
		.unwrap_or_default()
}

#[test]
fn a_slave_written_by_its_path_reaches_its_master() {
	let pair = openpty(SIZE).unwrap();
	assert_eq!(heard_through(&pair, b"ping"), b"ping");
}

#[test]
fn the_size_asked_for_is_the_masters() {
	let size = PtySize {
		rows: 27,
		cols: 93,
		..SIZE
	};
	let pair = openpty(size).unwrap();
	let got = pair.master.get_size().unwrap();
	assert_eq!((got.rows, got.cols), (27, 93));
}

/// FreeBSD's openpty(3) once failed here with EISDIR, or wired a master to
/// another thread's slave, when threads opened PTYs at the same time.
#[test]
fn ptys_opened_at_once_from_many_threads_each_reach_their_own_master() {
	thread::scope(|scope| {
		for thread in 0..8 {
			scope.spawn(move || {
				for round in 0..25 {
					let token = format!("<{thread}:{round}>");
					let pair = openpty(SIZE).unwrap_or_else(|e| panic!("{token}: {e}"));
					let heard = heard_through(&pair, token.as_bytes());
					assert_eq!(String::from_utf8_lossy(&heard), token);
				}
			});
		}
	});
}
