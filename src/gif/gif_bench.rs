//! GIF decoding speed on animations like those programs show inline:
//! `cargo test --release -- --ignored --nocapture gif_throughput`.
use std::{
	hint::black_box,
	time::{Duration, Instant},
};

use super::{
	Limits, decode,
	tests::{Image, image, write},
};

/// `frames` frames of a `side`-pixel square: a moving gradient over 64
/// colors, each frame only where it changed, kept or cleared after.
fn animation(side: u16, frames: u16) -> Vec<u8> {
	let palette: Vec<u32> = (0..64).map(|i| 0xff00_0000 | (i * 0x0004_0201)).collect();
	let images: Vec<Image> = (0..frames)
		.map(|f| {
			let (w, h) = (side - f % 8, side / 2);
			let indices = (0..u32::from(w) * u32::from(h))
				.map(|i| ((i / 3 + u32::from(f)) % 64) as u8)
				.collect();
			let mut img = image(w, h, indices);
			img.top = (f * 3) % (side / 2);
			img.control = Some((1 + (f % 2) as u8, 4, Some(63)));
			img
		})
		.collect();
	write(side, side, Some(&palette), &images)
}

fn best(run: &dyn Fn()) -> Duration {
	(0..5)
		.map(|_| {
			let start = Instant::now();
			for _ in 0..10 {
				run();
			}
			start.elapsed() / 10
		})
		.min()
		.unwrap()
}

#[test]
#[ignore = "benchmark: run with --release --ignored --nocapture"]
fn gif_throughput() {
	let limits = Limits {
		max_side: 1 << 14,
		max_pixels: 1 << 26,
		max_frames: 1000,
	};
	for (side, frames) in [(64, 30), (256, 30), (512, 10)] {
		let file = animation(side, frames);
		let gif = decode(&file, limits).unwrap();
		assert_eq!(gif.frames.len(), usize::from(frames));
		let time = best(&|| {
			black_box(decode(black_box(&file), limits).ok());
		});
		let pixels = usize::from(side) * usize::from(side) * usize::from(frames);
		let rate = (pixels * 4) as f64 / time.as_secs_f64() / 1e6;
		println!(
			"{side}×{side}, {frames} frames ({} KiB): {rate:.0} MB/s of frames",
			file.len() / 1024
		);
	}
}
