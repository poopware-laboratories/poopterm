//! Speed of the font engine against fontdue: loading a face, mapping
//! characters to glyphs, and rasterizing them. Run with
//! `cargo test --release -- --ignored --nocapture font_engine_speed`.
use std::{hint::black_box, time::Instant};

use super::font::Font;

const SIZES: [f32; 3] = [14.0, 17.0, 24.0];

/// Printable ASCII, the bulk of what a terminal draws.
fn chars() -> Vec<char> {
	(' '..='~').collect()
}

/// Nanoseconds per call of `run`, over enough calls to take about 0.3 s.
fn time(calls_per_run: usize, mut run: impl FnMut()) -> f64 {
	let start = Instant::now();
	let mut runs = 0;
	while start.elapsed().as_secs_f64() < 0.3 {
		run();
		runs += 1;
	}
	start.elapsed().as_nanos() as f64 / (runs * calls_per_run) as f64
}

fn report(name: &str, what: &str, ours: f64, theirs: f64) {
	println!(
		"{name:<22} {what:<10} ours {ours:>12.0} ns  fontdue {theirs:>12.0} ns  {:>5.2}x",
		theirs / ours
	);
}

#[test]
#[ignore = "benchmark"]
fn font_engine_speed() {
	let fonts: [(&str, Option<Vec<u8>>); 4] = [
		(
			"bundled",
			Some(include_bytes!("../../assets/font.ttf").to_vec()),
		),
		(
			"Source Code Pro (CFF)",
			std::fs::read("/usr/share/fonts/adobe-source-code-pro-fonts/SourceCodePro-Regular.otf").ok(),
		),
		(
			"Symbola (large)",
			std::fs::read("/usr/share/fonts/gdouros-symbola/Symbola.ttf").ok(),
		),
		(
			"Noto Sans Mono",
			std::fs::read("/usr/share/fonts/google-noto/NotoSansMono-Regular.ttf").ok(),
		),
	];
	let settings = fontdue::FontSettings::default;
	// What `load_fonts` reads at startup: the bundled font and up to eight
	// installed symbol fonts. Leaked once so loading borrows, not copies.
	let startup: Vec<(&'static [u8], u32)> = std::iter::once((super::BUNDLED, 0))
		.chain(
			super::symbols::symbol_fonts(&super::system_fonts(), 8)
				.into_iter()
				.filter_map(|(path, index)| Some((&*std::fs::read(path).ok()?.leak(), index))),
		)
		.collect();
	report(
		&format!("startup ({} fonts)", startup.len()),
		"load",
		time(1, || {
			for &(bytes, index) in &startup {
				black_box(Font::from_bytes(bytes, index).ok());
			}
		}),
		time(1, || {
			for &(bytes, index) in &startup {
				let settings = fontdue::FontSettings {
					collection_index: index,
					..settings()
				};
				black_box(fontdue::Font::from_bytes(bytes, settings).ok());
			}
		}),
	);
	let chars = chars();
	for (name, bytes) in fonts {
		let Some(bytes) = bytes else {
			println!("{name:<22} not installed");
			continue;
		};
		let bytes: &'static [u8] = bytes.leak();
		report(
			name,
			"load",
			time(1, || {
				black_box(Font::from_bytes(bytes, 0).unwrap());
			}),
			time(1, || {
				black_box(fontdue::Font::from_bytes(bytes, settings()).unwrap());
			}),
		);
		let ours = Font::from_bytes(bytes, 0).unwrap();
		let theirs = fontdue::Font::from_bytes(bytes, settings()).unwrap();
		report(
			name,
			"lookup",
			time(chars.len(), || {
				for &ch in &chars {
					black_box(ours.lookup_glyph_index(black_box(ch)));
				}
			}),
			time(chars.len(), || {
				for &ch in &chars {
					black_box(theirs.lookup_glyph_index(black_box(ch)));
				}
			}),
		);
		report(
			name,
			"metrics",
			time(chars.len(), || {
				for &ch in &chars {
					black_box(ours.metrics(ch, 17.0));
				}
			}),
			time(chars.len(), || {
				for &ch in &chars {
					black_box(theirs.metrics(ch, 17.0));
				}
			}),
		);
		let glyphs = chars.len() * SIZES.len();
		report(
			name,
			"rasterize",
			time(glyphs, || {
				for px in SIZES {
					for &ch in &chars {
						black_box(ours.rasterize(ch, px));
					}
				}
			}),
			time(glyphs, || {
				for px in SIZES {
					for &ch in &chars {
						black_box(theirs.rasterize(ch, px));
					}
				}
			}),
		);
	}
}

/// Where `rasterize` spends its time, stage by stage, on the bundled font.
#[test]
#[ignore = "benchmark"]
fn font_engine_stages() {
	use super::{
		face::Face,
		raster::{Path, Raster},
	};
	let face = Face::parse(include_bytes!("../../assets/font.ttf").as_slice().into(), 0).unwrap();
	let chars = chars();
	let n = chars.len();
	let glyphs: Vec<u16> = chars.iter().map(|&ch| face.glyph_index(ch)).collect();
	let ns = time(n, || {
		for &ch in &chars {
			black_box(face.glyph_index(black_box(ch)));
		}
	});
	println!("glyph_index   {ns:>8.0} ns");
	let ns = time(n, || {
		for &glyph in &glyphs {
			let mut path = Path::default();
			black_box(face.outline(glyph, &mut path));
			black_box(path);
		}
	});
	println!("outline       {ns:>8.0} ns");
	let paths: Vec<Path> = glyphs
		.iter()
		.map(|&glyph| {
			let mut path = Path::default();
			face.outline(glyph, &mut path);
			path
		})
		.collect();
	let ns = time(n, || {
		for path in &paths {
			black_box(path.bounds());
		}
	});
	println!("bounds        {ns:>8.0} ns");
	let scale = 17.0 / f32::from(face.units_per_em);
	let boxes: Vec<(usize, usize, f32, f32)> = paths
		.iter()
		.map(|path| {
			path.bounds().map_or((0, 0, 0.0, 0.0), |b| {
				let (l, bo) = ((b.x_min * scale).floor(), (b.y_min * scale).floor());
				let (r, t) = ((b.x_max * scale).ceil(), (b.y_max * scale).ceil());
				((r - l) as usize, (t - bo) as usize, l, t)
			})
		})
		.collect();
	let ns = time(n, || {
		for &(w, h, _, _) in &boxes {
			black_box(Raster::new(w, h));
		}
	});
	println!("raster alloc  {ns:>8.0} ns");
	let ns = time(n, || {
		for (path, &(w, h, l, t)) in paths.iter().zip(&boxes) {
			let mut raster = Raster::new(w, h);
			raster.draw(path, scale, l, t);
			black_box(&raster);
		}
	});
	println!("alloc + draw  {ns:>8.0} ns");
	let rasters: Vec<Raster> = paths
		.iter()
		.zip(&boxes)
		.map(|(path, &(w, h, l, t))| {
			let mut raster = Raster::new(w, h);
			raster.draw(path, scale, l, t);
			raster
		})
		.collect();
	let ns = time(n, || {
		for raster in &rasters {
			black_box(raster.coverage());
		}
	});
	println!("coverage      {ns:>8.0} ns");
}
