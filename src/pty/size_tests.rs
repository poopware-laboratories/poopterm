//! The size a session gives its PTY: set when the grid or its cells'
//! pixels change, never again for the same size, and remembered only once
//! the kernel took it. Checked on real pairs, read back as programs do.
use super::{
	device::{self, WindowSize},
	size::SentSize,
};
use std::fs::File;

fn pair(size: WindowSize) -> device::Pair {
	device::open(size).unwrap()
}

/// What a program on the slave reads with `TIOCGWINSZ`.
fn seen(pair: &device::Pair) -> WindowSize {
	device::size(&pair.slave).unwrap()
}

fn size(cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) -> WindowSize {
	WindowSize {
		cols,
		rows,
		pixel_width,
		pixel_height,
	}
}

#[test]
fn the_size_a_pair_opened_with_is_not_set_again() {
	let opened = WindowSize::of(100, 30, (9, 18));
	let pty = pair(opened);
	let sent = SentSize::new(opened);
	assert!(!sent.set(&pty.master, (100, 30), (9, 18)).unwrap());
	assert_eq!(seen(&pty), size(100, 30, 900, 540));
}

#[test]
fn programs_see_the_cells_pixels() {
	let pty = pair(WindowSize::of(80, 24, (0, 0)));
	let sent = SentSize::new(WindowSize::of(80, 24, (0, 0)));
	assert_eq!(seen(&pty).pixel_width, 0, "unknown before the window is");
	assert!(sent.set(&pty.master, (80, 24), (10, 20)).unwrap());
	assert_eq!(seen(&pty), size(80, 24, 800, 480));
}

#[test]
fn a_zoom_that_keeps_the_grid_still_sets_the_new_pixels() {
	let pty = pair(WindowSize::default());
	let sent = SentSize::default();
	assert!(sent.set(&pty.master, (80, 24), (10, 20)).unwrap());
	assert!(sent.set(&pty.master, (80, 24), (12, 24)).unwrap());
	assert_eq!(seen(&pty), size(80, 24, 960, 576));
	assert!(
		!sent.set(&pty.master, (80, 24), (12, 24)).unwrap(),
		"unchanged"
	);
}

#[test]
fn a_new_grid_with_the_same_cells_is_set() {
	let pty = pair(WindowSize::default());
	let sent = SentSize::default();
	assert!(sent.set(&pty.master, (80, 24), (10, 20)).unwrap());
	assert!(sent.set(&pty.master, (120, 24), (10, 20)).unwrap());
	assert!(sent.set(&pty.master, (120, 40), (10, 20)).unwrap());
	assert_eq!(seen(&pty), size(120, 40, 1200, 800));
	// Back to an earlier size is a change too.
	assert!(sent.set(&pty.master, (80, 24), (10, 20)).unwrap());
	assert_eq!(seen(&pty), size(80, 24, 800, 480));
}

#[test]
fn sizes_that_saturate_to_the_same_values_are_not_set_twice() {
	let pty = pair(WindowSize::default());
	let sent = SentSize::default();
	// Both carry 65535 pixels a side, the most the ioctl holds.
	assert!(sent.set(&pty.master, (1000, 1000), (70, 70)).unwrap());
	assert!(!sent.set(&pty.master, (1000, 1000), (80, 80)).unwrap());
	assert_eq!(seen(&pty), size(1000, 1000, u16::MAX, u16::MAX));
}

#[test]
fn each_field_counts_on_its_own() {
	// Packing must keep the four fields apart: swapping any two is a change.
	let pty = pair(WindowSize::default());
	let sent = SentSize::default();
	assert!(sent.set(&pty.master, (2, 1), (1, 1)).unwrap());
	assert!(sent.set(&pty.master, (1, 2), (1, 1)).unwrap());
	assert!(sent.set(&pty.master, (1, 2), (2, 1)).unwrap());
	assert!(sent.set(&pty.master, (1, 2), (1, 2)).unwrap());
	assert_eq!(seen(&pty), size(1, 2, 1, 4));
}

#[test]
fn a_size_the_kernel_refused_is_not_remembered() {
	let sent = SentSize::default();
	let not_a_terminal = File::open("/dev/null").unwrap();
	assert!(sent.set(&not_a_terminal, (80, 24), (10, 20)).is_err());
	// The same size still goes to a real terminal afterwards.
	let pty = pair(WindowSize::default());
	assert!(sent.set(&pty.master, (80, 24), (10, 20)).unwrap());
	assert_eq!(seen(&pty), size(80, 24, 800, 480));
}

#[test]
fn wide_fields_do_not_spill_into_their_neighbours() {
	// With fields narrower than 16 bits, the column's bit would land on
	// the row's bit `k`: one column of 2^k rows would pack as one of none.
	let pty = pair(WindowSize::default());
	for k in [4, 8, 12, 15] {
		let sent = SentSize::default();
		assert!(
			sent.set(&pty.master, (1, 1 << k), (0, 0)).unwrap(),
			"2^{k} rows"
		);
		assert!(
			sent.set(&pty.master, (1, 0), (0, 0)).unwrap(),
			"then none, after 2^{k}"
		);
		assert_eq!((seen(&pty).cols, seen(&pty).rows), (1, 0));
	}
}

/// Lines a shell on `pty`'s slave prints, as they arrive.
fn lines_from(pty: device::Pair) -> (super::spawn::Child, std::sync::mpsc::Receiver<String>) {
	use std::io::{BufRead, BufReader};
	// Prints "winch" for each SIGWINCH, after saying it is ready.
	let script = "trap 'echo winch' WINCH; echo ready; while :; do sleep 0.02; done";
	let command = super::spawn::Command::new("/bin/sh").arg("-c").arg(script);
	let child = super::spawn::spawn(&command, &pty.slave).unwrap();
	drop(pty.slave);
	let (send, lines) = std::sync::mpsc::channel();
	let screen = BufReader::new(File::from(pty.master));
	std::thread::spawn(move || {
		for line in screen.lines() {
			let Ok(line) = line else { return };
			if send.send(line.trim_end().to_owned()).is_err() {
				return;
			}
		}
	});
	(child, lines)
}

#[test]
fn the_program_is_signalled_once_per_change_and_not_for_the_same_size() {
	use std::time::Duration;
	let pty = pair(WindowSize::of(80, 24, (0, 0)));
	let master = pty.master.try_clone().unwrap();
	let sent = SentSize::new(WindowSize::of(80, 24, (0, 0)));
	let (mut child, lines) = lines_from(pty);
	let next = |wait| lines.recv_timeout(Duration::from_millis(wait)).ok();
	assert_eq!(next(5000).as_deref(), Some("ready"));
	// The cells' pixels arrive: a change, though the grid is the same.
	assert!(sent.set(&master, (80, 24), (10, 20)).unwrap());
	assert_eq!(next(5000).as_deref(), Some("winch"));
	// The same size again sends nothing.
	assert!(!sent.set(&master, (80, 24), (10, 20)).unwrap());
	assert_eq!(next(300), None, "no signal for an unchanged size");
	assert!(sent.set(&master, (100, 24), (10, 20)).unwrap());
	assert_eq!(next(5000).as_deref(), Some("winch"));
	child.hang_up().unwrap();
	child.wait().unwrap();
}
