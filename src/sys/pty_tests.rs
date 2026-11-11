//! PTY pairs: sizes seen from both sides and by programs, the bytes the
//! kernel passes each way, and descriptors that stay ours.
use super::*;
use crate::bench::corpus::Rng;
use std::{
	fs::File,
	io::{Read, Write},
	os::fd::AsRawFd,
	path::Path,
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
fn programs_see_the_size_asked() {
	// Within 2..30000: busybox's stty (Alpine) reports 0, 1 and anything from
	// 30000 up as its default, 80 by 24.
	for (cols, rows) in [(2, 2), (80, 24), (7, 29_999), (29_999, 2), (12_345, 678)] {
		let pair = open(WindowSize {
			cols,
			rows,
			pixel_width: 1,
			pixel_height: 2,
		})
		.unwrap();
		assert_eq!(stty_size(&pair.slave), format!("{rows} {cols}"));
	}
}

#[test]
fn a_resize_reaches_the_slave() {
	let pair = open(cells(80, 24)).unwrap();
	for size in sizes() {
		set_size(&pair.master, size).unwrap();
		assert_eq!(super::size(&pair.slave).unwrap(), size);
	}
	set_size(&pair.master, cells(132, 43)).unwrap();
	assert_eq!(stty_size(&pair.slave), "43 132");
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

/// What a new pair's master gives for `data` written to its slave, read
/// until quiet.
fn output(data: &[u8]) -> Vec<u8> {
	let pair = open(cells(80, 24)).unwrap();
	let reading = {
		let out = File::from(pair.master.try_clone().unwrap());
		thread::spawn(move || drain(out))
	};
	File::from(pair.slave).write_all(data).unwrap();
	reading.join().unwrap()
}

/// `data` as the kernel's default output processing passes it on: each
/// newline gains a carriage return, everything else is left alone.
fn cooked(data: &[u8]) -> Vec<u8> {
	data
		.iter()
		.flat_map(|byte| {
			if *byte == b'\n' {
				&b"\r\n"[..]
			} else {
				std::slice::from_ref(byte)
			}
		})
		.copied()
		.collect()
}

#[test]
fn output_comes_out_of_the_master_with_only_newlines_changed() {
	let mut rng = Rng::new(7);
	for seed in 0..8 {
		let len = 1 + rng.below(3000);
		let data: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
		assert_eq!(output(&data), cooked(&data), "seed {seed}, {len} bytes");
	}
}

#[test]
fn every_byte_value_passes_through_and_newlines_gain_a_carriage_return() {
	let data: Vec<u8> = (0..=255).collect();
	let got = output(&data);
	assert_eq!(got.len(), 257);
	assert_eq!(got, cooked(&data));
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
	// A regular file: devices such as /dev/null answer ENODEV on macOS.
	let file = File::open("Cargo.toml").unwrap();
	let error = set_size(&file, cells(80, 24)).unwrap_err();
	assert_eq!(error.raw_os_error(), Some(libc::ENOTTY));
	assert_eq!(size(&file).unwrap_err().raw_os_error(), Some(libc::ENOTTY));
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

#[test]
fn output_larger_than_the_kernels_buffer_arrives_whole() {
	// Far past what a PTY buffers (4 KiB on macOS, 64 KiB or so elsewhere),
	// so the writer waits for the reader.
	let data = b"line of output\n".repeat(20_000);
	let got = output(&data);
	assert_eq!(
		got.len(),
		data.len() + 20_000,
		"each newline gains a carriage return"
	);
	assert_eq!(got, b"line of output\r\n".repeat(20_000));
}

#[test]
fn typed_input_reaches_the_slave_as_an_edited_line_and_echoes() {
	let typed = b"echo hi\tthere\x7f\x7fre\n";
	let pair = open(cells(80, 24)).unwrap();
	let echoing = {
		let echo = File::from(pair.master.try_clone().unwrap());
		thread::spawn(move || drain(echo))
	};
	File::from(pair.master.try_clone().unwrap())
		.write_all(typed)
		.unwrap();
	// Line editing hands the slave one finished line: the two erases took
	// back "re" of "there".
	let mut line = vec![0; 64];
	let n = (&File::from(pair.slave)).read(&mut line).unwrap();
	assert_eq!(&line[..n], b"echo hi\tthere\n");
	// The echo shows what was typed, its newline cooked like any output.
	let echo = echoing.join().unwrap();
	assert!(echo.starts_with(b"echo hi"), "{echo:?}");
	assert!(echo.ends_with(b"re\r\n"), "{echo:?}");
}

#[test]
fn the_master_ends_once_the_slave_closes() {
	let ours = open(cells(80, 24)).unwrap();
	let mut out = File::from(ours.master.try_clone().unwrap());
	let mut slave = File::from(ours.slave);
	slave.write_all(b"last\n").unwrap();
	// Read before the close: macOS and FreeBSD drop what the master has not
	// read once the slave closes, Linux keeps it.
	let mut buf = [0; 16];
	let n = out.read(&mut buf).unwrap();
	assert_eq!(&buf[..n], b"last\r\n");
	drop(slave);
	// Then Linux answers EIO and the BSDs end of file; a reader must stop at
	// either.
	match out.read(&mut buf) {
		Ok(0) => {}
		Err(error) => assert_eq!(error.raw_os_error(), Some(libc::EIO)),
		Ok(n) => panic!("{n} more bytes"),
	}
}

#[test]
fn pairs_opened_at_once_from_many_threads_have_their_own_slaves() {
	let opening: Vec<_> = (0..16)
		.map(|_| {
			thread::spawn(|| {
				(0..8)
					.map(|_| open(cells(80, 24)).unwrap())
					.collect::<Vec<_>>()
			})
		})
		.collect();
	let pairs: Vec<Pair> = opening
		.into_iter()
		.flat_map(|thread| thread.join().unwrap())
		.collect();
	let mut paths: Vec<_> = pairs.iter().map(|pair| pair.path.clone()).collect();
	paths.sort();
	paths.dedup();
	assert_eq!(paths.len(), pairs.len());
	// And each path is its own pair's slave: a size set on the master shows
	// at the path.
	for (n, pair) in pairs.iter().enumerate() {
		set_size(&pair.master, cells(n as u16 + 1, 7)).unwrap();
		let slave = File::open(&pair.path).unwrap();
		assert_eq!(super::size(&slave).unwrap(), cells(n as u16 + 1, 7));
	}
}

#[test]
fn a_size_set_on_the_slave_shows_on_the_master() {
	let pair = open(cells(80, 24)).unwrap();
	set_size(&pair.slave, cells(100, 30)).unwrap();
	assert_eq!(size(&pair.master).unwrap(), cells(100, 30));
}

#[test]
fn slaves_live_where_the_system_puts_terminals() {
	let pair = open(cells(80, 24)).unwrap();
	let name = pair.path.file_name().unwrap().to_str().unwrap();
	// /dev/pts/N on Linux and FreeBSD, /dev/ttysNNN on macOS.
	if cfg!(target_os = "macos") {
		assert_eq!(pair.path.parent(), Some(Path::new("/dev")));
		assert!(name.starts_with("ttys"), "{name}");
	} else {
		assert_eq!(pair.path.parent(), Some(Path::new("/dev/pts")));
		assert!(name.parse::<u32>().is_ok(), "{name}");
	}
}

#[test]
fn a_closed_master_hangs_the_slave_up() {
	let pair = open(cells(80, 24)).unwrap();
	let mut slave = File::from(pair.slave);
	drop(pair.master);
	// What a program sees when its terminal goes away: writes fail.
	let error = slave.write_all(b"anyone?").unwrap_err();
	let hung_up = [Some(libc::EIO), Some(libc::ENXIO)];
	assert!(hung_up.contains(&error.raw_os_error()), "{error}");
}

#[test]
fn a_grid_size_reaches_programs_with_its_pixels() {
	let pair = open(WindowSize::of(120, 40, (10, 21))).unwrap();
	assert_eq!(
		size(&pair.slave).unwrap(),
		WindowSize::of(120, 40, (10, 21))
	);
	assert_eq!(size(&pair.slave).unwrap().pixel_height, 840);
}

#[test]
fn writing_nothing_gives_nothing() {
	assert_eq!(output(b""), b"");
}

#[test]
fn check_passes_results_and_turns_negatives_into_errno() {
	for result in [0, 7, i32::MAX] {
		assert_eq!(check(result).unwrap(), result);
	}
	// SAFETY: closing a descriptor that is never open only sets errno.
	let failed = unsafe { libc::close(-1) };
	assert_eq!(failed, -1);
	assert_eq!(
		check(failed).err().unwrap().raw_os_error(),
		Some(libc::EBADF)
	);
	// Any negative result is a failure, not only -1, and reports `errno`
	// as the call left it.
	// SAFETY: as above.
	let failed = unsafe { libc::close(-1) };
	assert_eq!(
		check(failed - 1).err().unwrap().raw_os_error(),
		Some(libc::EBADF)
	);
	assert!(check(i32::MIN).is_err());
	// `errno` still says EBADF here; a success must not read it.
	assert_eq!(check(3).unwrap(), 3);
}

/// Probe, never merged: when a session leader outlives its closed PTY, can
/// its number go to a new pair while the leader lives, and does a program on
/// the new pair still work after the old leader exits? Nothing is left
/// unread, so no close waits for output to drain.
#[cfg(target_os = "macos")]
#[test]
fn probe_macos_revoke_after_reuse() {
	use crate::sys::spawn::{Command as Program, spawn};
	use std::sync::{Arc, Mutex};
	use std::time::Instant;
	let mut report = Vec::new();
	for trial in 0..10 {
		let step = Arc::new(Mutex::new(String::from("start")));
		let at = |step: &Arc<Mutex<String>>, name: &str| *step.lock().unwrap() = name.to_owned();
		let (done, finished) = mpsc::channel();
		let shared = Arc::clone(&step);
		thread::spawn(move || {
			let step = shared;
			let old = open(cells(80, 24)).unwrap();
			let old_path = old.path.clone();
			let mut sleeper = spawn(&Program::new("sh").arg("-c").arg("sleep 0.5"), &old.slave).unwrap();
			drop(old);
			let begun = Instant::now();
			let mut opens = 0;
			let mut hit = None;
			while begun.elapsed() < Duration::from_secs(2) {
				let pair = open(cells(80, 24)).unwrap();
				opens += 1;
				if pair.path == old_path {
					hit = Some(pair);
					break;
				}
			}
			let reused_after = begun.elapsed().as_millis();
			let sleeper_alive = sleeper.try_wait().map(|exit| exit.is_none());
			at(&step, "wait sleeper");
			let exit = sleeper.wait();
			let line = match hit {
				None => format!("reused=false opens={opens} sleeper={exit:?}"),
				Some(pair) => {
					thread::sleep(Duration::from_millis(100));
					// SAFETY: asks about a descriptor we hold.
					let tty = unsafe { libc::isatty(pair.slave.as_raw_fd()) };
					at(&step, "spawn on reused pair");
					let started = spawn(
						&Program::new("sh").arg("-c").arg("printf ok; exit 7"),
						&pair.slave,
					)
					.map_err(|e| e.raw_os_error());
					at(&step, "read reused master");
					let mut master = File::from(pair.master);
					let mut shown = [0; 16];
					let read = master
						.read(&mut shown)
						.map(|n| String::from_utf8_lossy(&shown[..n]).into_owned());
					at(&step, "wait on reused pair");
					let ended = started.map(|mut child| child.wait().map_err(|e| e.raw_os_error()));
					format!(
						"reused=true after={reused_after}ms opens={opens} leader_alive_then={sleeper_alive:?} isatty={tty} read={read:?} program={ended:?}"
					)
				}
			};
			done.send(line).unwrap();
		});
		match finished.recv_timeout(Duration::from_secs(10)) {
			Ok(line) => report.push(line),
			Err(_) => {
				report.push(format!("trial {trial} STUCK at: {}", step.lock().unwrap()));
				break;
			}
		}
	}
	panic!("PROBE\n{}", report.join("\n"));
}
