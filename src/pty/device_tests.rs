//! PTY pairs of our own against portable-pty's: the same sizes seen from
//! both sides, the same bytes out of the same writes, and descriptors that
//! stay ours.
use super::*;
use crate::bench::corpus::Rng;
use portable_pty::{PtySize, native_pty_system};
use std::{
	fs::File,
	io::{Read, Write},
	os::fd::AsRawFd,
	process::{Command, Stdio},
	sync::mpsc,
	thread,
	time::Duration,
};

fn cells(cols: u16, rows: u16) -> WindowSize {
	WindowSize {
		cols,
		rows,
		..WindowSize::default()
	}
}

/// What `stty size` run on `tty` prints: what programs see.
fn stty_size(tty: &impl std::os::fd::AsFd) -> String {
	let stdin = File::from(tty.as_fd().try_clone_to_owned().unwrap());
	let run = Command::new("stty")
		.arg("size")
		.stdin(stdin)
		.output()
		.unwrap();
	assert!(
		run.status.success(),
		"{}",
		String::from_utf8_lossy(&run.stderr)
	);
	String::from_utf8(run.stdout).unwrap().trim().to_owned()
}

/// portable-pty's pair of `size`: its master, and its slave opened by path.
fn oracle(size: WindowSize) -> (Box<dyn portable_pty::MasterPty + Send>, File) {
	let pair = native_pty_system()
		.openpty(PtySize {
			rows: size.rows,
			cols: size.cols,
			pixel_width: size.pixel_width,
			pixel_height: size.pixel_height,
		})
		.unwrap();
	let path = pair.master.tty_name().unwrap();
	let slave = File::options().read(true).write(true).open(path).unwrap();
	(pair.master, slave)
}

/// Sizes to try: the edges, then seeded random ones.
fn sizes() -> Vec<WindowSize> {
	let mut sizes = vec![
		WindowSize::default(),
		cells(1, 1),
		cells(80, 24),
		WindowSize {
			cols: u16::MAX,
			rows: u16::MAX,
			pixel_width: u16::MAX,
			pixel_height: u16::MAX,
		},
	];
	let mut rng = Rng::new(188);
	let mut any = || rng.next() as u16;
	for _ in 0..32 {
		sizes.push(WindowSize {
			cols: any(),
			rows: any(),
			pixel_width: any(),
			pixel_height: any(),
		});
	}
	sizes
}

#[test]
fn a_new_pair_has_the_size_asked_on_both_sides() {
	for asked in sizes() {
		let pair = open(asked).unwrap();
		assert_eq!(size(&pair.slave).unwrap(), asked);
		assert_eq!(size(&pair.master).unwrap(), asked);
	}
}

#[test]
fn programs_see_the_same_size_as_on_portable_ptys() {
	for size in sizes().into_iter().take(12) {
		let ours = open(size).unwrap();
		let (_master, theirs) = oracle(size);
		assert_eq!(stty_size(&ours.slave), stty_size(&theirs), "{size:?}");
		assert_eq!(
			super::size(&ours.slave).unwrap(),
			super::size(&theirs).unwrap()
		);
	}
}

#[test]
fn a_resize_reaches_the_slave_as_portable_ptys_do() {
	let pair = open(cells(80, 24)).unwrap();
	let (master, theirs) = oracle(cells(80, 24));
	for size in sizes() {
		set_size(&pair.master, size).unwrap();
		master
			.resize(PtySize {
				rows: size.rows,
				cols: size.cols,
				pixel_width: size.pixel_width,
				pixel_height: size.pixel_height,
			})
			.unwrap();
		assert_eq!(super::size(&pair.slave).unwrap(), size);
		assert_eq!(super::size(&theirs).unwrap(), size);
		assert_eq!(master.get_size().unwrap().cols, size.cols);
	}
}

#[test]
fn the_path_names_the_slave() {
	let pair = open(cells(80, 24)).unwrap();
	set_size(&pair.master, cells(132, 43)).unwrap();
	let again = File::options()
		.read(true)
		.write(true)
		.open(&pair.path)
		.unwrap();
	assert_eq!(super::size(&again).unwrap(), cells(132, 43));
	assert!(pair.path.starts_with("/dev"), "{:?}", pair.path);
}

/// Everything `master` gives until it has been quiet for a while.
fn drain(master: impl Read + Send + 'static) -> Vec<u8> {
	let (send, heard) = mpsc::channel();
	thread::spawn(move || {
		let (mut master, mut buf) = (master, [0; 4096]);
		while let Ok(n @ 1..) = master.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	let mut all = Vec::new();
	while let Ok(bytes) = heard.recv_timeout(Duration::from_millis(300)) {
		all.extend_from_slice(&bytes);
	}
	all
}

#[test]
fn output_comes_out_of_the_master_as_from_portable_ptys() {
	let mut rng = Rng::new(7);
	for seed in 0..8 {
		let len = 1 + rng.below(3000);
		let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
		let ours = open(cells(80, 24)).unwrap();
		let (master, mut theirs) = oracle(cells(80, 24));
		let (ours_out, theirs_out) = (
			File::from(ours.master.try_clone().unwrap()),
			master.try_clone_reader().unwrap(),
		);
		let reading = thread::spawn(move || drain(ours_out));
		File::from(ours.slave).write_all(&data).unwrap();
		let theirs_reading = thread::spawn(move || drain(theirs_out));
		theirs.write_all(&data).unwrap();
		let (ours_got, theirs_got) = (reading.join().unwrap(), theirs_reading.join().unwrap());
		assert!(!ours_got.is_empty(), "seed {seed}");
		assert_eq!(ours_got, theirs_got, "seed {seed}, {len} bytes");
	}
}

#[test]
fn neither_side_reaches_a_child_process() {
	let pair = open(cells(80, 24)).unwrap();
	for fd in [pair.master.as_raw_fd(), pair.slave.as_raw_fd()] {
		// Closed on exec, the descriptor is not there for the shell to use.
		let script = format!("stty size <&{fd}");
		let run = Command::new("sh")
			.args(["-c", &script])
			.stdin(Stdio::null())
			.stderr(Stdio::null())
			.output()
			.unwrap();
		assert!(!run.status.success(), "fd {fd} was inherited");
	}
	// The same check passes for a descriptor that is inherited.
	let run = Command::new("sh")
		.args(["-c", "stty size <&0"])
		.stdin(File::from(pair.slave.try_clone().unwrap()))
		.output()
		.unwrap();
	assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "24 80");
}

#[test]
fn dropped_pairs_give_their_descriptors_back() {
	// Leaking two descriptors a pair would run out within the default
	// limits (1024 descriptors on Linux, 511 PTYs on macOS).
	for _ in 0..1500 {
		drop(open(cells(10, 5)).unwrap());
	}
}

#[test]
fn something_that_is_not_a_terminal_has_no_size() {
	let null = File::options().write(true).open("/dev/null").unwrap();
	let error = set_size(&null, cells(80, 24)).unwrap_err();
	assert_eq!(error.raw_os_error(), Some(libc::ENOTTY));
	assert_eq!(size(&null).unwrap_err().raw_os_error(), Some(libc::ENOTTY));
}

#[test]
fn a_grid_size_counts_its_pixels() {
	let size = WindowSize::of(80, 24, (9, 18));
	assert_eq!(
		size,
		WindowSize {
			cols: 80,
			rows: 24,
			pixel_width: 720,
			pixel_height: 432,
		}
	);
	assert_eq!(WindowSize::of(0, 0, (9, 18)), WindowSize::default());
	assert_eq!(WindowSize::of(80, 24, (0, 0)), cells(80, 24));
}

#[test]
fn a_grid_size_saturates_at_what_the_ioctl_carries() {
	let huge = WindowSize::of(usize::MAX, 70_000, (usize::MAX, 2));
	assert_eq!(
		huge,
		WindowSize {
			cols: u16::MAX,
			rows: u16::MAX,
			pixel_width: u16::MAX,
			pixel_height: u16::MAX,
		}
	);
	// 1000 columns of 66 pixels pass the limit, 65 stay within it.
	assert_eq!(WindowSize::of(1000, 1, (66, 1)).pixel_width, u16::MAX);
	assert_eq!(WindowSize::of(1000, 1, (65, 1)).pixel_width, 65_000);
}
