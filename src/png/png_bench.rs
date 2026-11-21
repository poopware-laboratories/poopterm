//! PNG decoding throughput against the `image` crate, on images like the
//! kitty benchmark workloads: `cargo test --release -- --ignored
//! --nocapture png_throughput`.
use std::{
	hint::black_box,
	time::{Duration, Instant},
};

use super::{decode, tests::oracle};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

/// A `side`-pixel square of RGBA: like a screenshot (gradients and
/// edges), or like the kitty benchmark workloads (`busy`: an XOR pattern
/// that compresses poorly and filters with Paeth).
fn screenshot(side: u32, busy: bool) -> Vec<u8> {
	let pixels = (0..side * side).flat_map(|i| {
		let (x, y) = (i % side, i / side);
		let blue = if busy {
			(x ^ y) as u8
		} else {
			((x / 16) ^ (y / 16)) as u8 * 40
		};
		[(x * 255 / side) as u8, (y * 255 / side) as u8, blue, 255]
	});
	let raw: Vec<u8> = pixels.collect();
	let mut png = Vec::new();
	PngEncoder::new(&mut png)
		.write_image(&raw, side, side, ExtendedColorType::Rgba8)
		.unwrap();
	png
}

/// The best of five runs, each decoding about 64 MiB of pixels.
fn best(pixels: usize, run: &dyn Fn()) -> Duration {
	let reps = ((1 << 24) / pixels.max(1)).max(1);
	(0..5)
		.map(|_| {
			let start = Instant::now();
			for _ in 0..reps {
				run();
			}
			start.elapsed() / reps as u32
		})
		.min()
		.unwrap()
}

#[test]
#[ignore = "benchmark: run with --release --ignored --nocapture"]
fn png_throughput() {
	for (side, busy) in [
		(64, false),
		(512, false),
		(1024, false),
		(32, true),
		(128, true),
		(256, true),
	] {
		let png = screenshot(side, busy);
		let pixels = (side * side) as usize;
		assert_eq!(
			decode(&png, 1 << 14, 1 << 26).ok(),
			oracle(&png),
			"same pixels first"
		);
		let ours = best(pixels, &|| {
			black_box(decode(black_box(&png), 1 << 14, 1 << 26).ok());
		});
		let theirs = best(pixels, &|| {
			black_box(oracle(black_box(&png)));
		});
		let rate = |time: Duration| (pixels * 4) as f64 / time.as_secs_f64() / 1e6;
		println!(
			"{side}×{side}{} ({} KiB): ours {:.0} MB/s, image crate {:.0} MB/s, {:.2}×",
			if busy { " busy" } else { "" },
			png.len() / 1024,
			rate(ours),
			rate(theirs),
			theirs.as_secs_f64() / ours.as_secs_f64(),
		);
	}
}
