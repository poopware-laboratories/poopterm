//! Headless benchmarks over a corpus of real-world workloads.
//!
//! `poopterm --benchmark` feeds every workload through the parser and
//! terminal, with and without rendering frames, several times, and prints
//! the median MB/s with its spread. It needs no window, so it is also the
//! training run for profile-guided optimization.
//! `poopterm --bench-corpus NAME` prints a workload's bytes, so other
//! terminals can be measured on identical input (`scripts/bench-throughput.py`).
pub(crate) mod corpus;
pub(crate) mod images;
pub(crate) mod report;
pub(crate) mod stats;
pub(crate) mod table;

use crate::{
	parser::Parser,
	render::{
		glyphs::FontSet,
		grid::{GridView, draw_grid},
		painter::Painter,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};
use corpus::{Size, WORKLOADS, Workload};
use report::{Stage, report};
use std::{error::Error, io::Write, time::Instant};
use table::Style;

/// Bytes per `advance` call, like one PTY read.
const READ: usize = 64 << 10;
/// Timed runs per workload and stage by default; the median is reported.
const REPEAT: usize = 3;
/// Bytes between rendered frames: a 200 MB/s flood drawn at 30 fps is about
/// 6.7 MB per frame; interactive programs draw far more often. This is in
/// between, and keeps rendering in the PGO profile.
const FRAME_EVERY: usize = 1 << 20;
const CELL: (usize, usize) = (10, 20);

pub(crate) struct Options {
	/// Bytes per workload.
	bytes: usize,
	size: Size,
	only: Vec<String>,
	render: bool,
	feed: Feed,
	/// Timed runs of each stage, after one untimed warm-up.
	repeat: usize,
	style: Style,
}

/// How output reaches the terminal: floods arrive in large reads, while
/// interactive programs (key echo, prompts) write a few bytes at a time and
/// get a frame for each.
#[derive(Clone, Copy)]
pub(crate) struct Feed {
	/// Bytes per `advance` call, like one PTY read.
	pub read: usize,
	/// Bytes after which a frame is drawn, checked after each read.
	pub frame_every: usize,
}

impl Feed {
	pub(crate) const FLOOD: Self = Self {
		read: READ,
		frame_every: FRAME_EVERY,
	};
}

pub(crate) fn options(args: impl Iterator<Item = String>) -> Result<Options, Box<dyn Error>> {
	let mut options = Options {
		bytes: 16 << 20,
		size: Size {
			cols: 120,
			rows: 40,
		},
		only: Vec::new(),
		render: true,
		feed: Feed::FLOOD,
		repeat: REPEAT,
		style: Style::Text,
	};
	let mut args = args.peekable();
	while let Some(arg) = args.next() {
		let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
		match arg.as_str() {
			"--mib" => options.bytes = value()?.parse::<usize>()?.clamp(1, 4096) << 20,
			"--kib" => options.bytes = value()?.parse::<usize>()?.clamp(1, 1 << 22) << 10,
			"--size" => {
				let value = value()?;
				let (cols, rows) = value.split_once('x').ok_or("--size is COLSxROWS")?;
				options.size = Size {
					cols: cols.parse::<usize>()?.clamp(2, 1000),
					rows: rows.parse::<usize>()?.clamp(2, 1000),
				};
			}
			"--read" => options.feed.read = value()?.parse::<usize>()?.clamp(1, 1 << 20),
			"--frame-every" => options.feed.frame_every = value()?.parse::<usize>()?.clamp(1, 1 << 30),
			"--only" => options.only.extend(value()?.split(',').map(str::to_owned)),
			"--no-render" => options.render = false,
			"--repeat" => options.repeat = value()?.parse::<usize>()?.clamp(1, 50),
			"--markdown" => options.style = Style::Markdown,
			"--list" => {
				for workload in WORKLOADS {
					println!("{:<11} {}", workload.name, workload.about);
				}
				std::process::exit(0);
			}
			_ => {
				let known = crate::cli::documented("BENCHMARK OPTIONS");
				return Err(crate::cli::unknown("benchmark option", &arg, &known).into());
			}
		}
	}
	for name in &options.only {
		corpus::find(name).ok_or_else(|| format!("unknown workload: {name}; try --list"))?;
	}
	Ok(options)
}

fn fonts() -> FontSet {
	let font =
		crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).expect("bundled font is valid");
	FontSet::new(font, Vec::new())
}

/// Feeds `data` to a terminal whose history is full, as after real use, in
/// reads of `feed.read`; draws a frame every `feed.frame_every` bytes when
/// `fonts` is given. Returns MB/s.
pub(crate) fn measure(
	data: &[u8],
	size: Size,
	feed_by: Feed,
	mut fonts: Option<&mut FontSet>,
) -> f64 {
	let theme = Theme::from_kind(ThemeKind::Poop);
	let mut term = Term::new(size.cols, size.rows, theme, "/".into());
	(term.cell_width, term.cell_height) = CELL;
	let mut parser = Parser::new();
	// Costs that grow with history (copying it, say) show up only when there is some.
	let line = format!("{}\r\n", "x".repeat(size.cols / 2));
	let lines = term.scrollback_limit + size.rows;
	parser.advance(&mut term, line.repeat(lines).as_bytes());
	term.responses.clear();
	let mut frame = Frame::default();
	let mut pixels = vec![0u32; size.cols * CELL.0 * size.rows * CELL.1];
	let start = Instant::now();
	feed(&mut parser, &mut term, data, feed_by, |term| {
		if let Some(fonts) = fonts.as_deref_mut() {
			term.snapshot(&mut frame);
			draw(&mut pixels, fonts, &frame, theme.bg);
		}
	});
	data.len() as f64 / start.elapsed().as_secs_f64() / 1e6
}

/// Advances `term` through `data` in reads of `feed.read`, dropping its
/// replies, and calls `frame` once `feed.frame_every` bytes have arrived
/// since the last one.
fn feed(
	parser: &mut Parser,
	term: &mut Term,
	data: &[u8],
	feed: Feed,
	mut frame: impl FnMut(&mut Term),
) {
	let mut since_frame = 0;
	for chunk in data.chunks(feed.read) {
		parser.advance(term, chunk);
		term.responses.clear();
		since_frame += chunk.len();
		if since_frame >= feed.frame_every {
			since_frame = 0;
			frame(term);
		}
	}
}

fn draw(pixels: &mut [u32], fonts: &mut FontSet, frame: &Frame, bg: u32) {
	let (width, height) = (frame.cols * CELL.0, frame.rows * CELL.1);
	let mut painter = Painter::new(pixels, width, height);
	painter.fill(bg);
	let view = GridView {
		frame,
		origin: (0, 0),
		cell: CELL,
		px: 16.0,
		ascent: 15,
		scale: 1,
		default_bg: bg,
		link_color: 0x89b4fa,
		selection_colors: (0x1e1e2e, 0x89b4fa),
		match_colors: (bg, 0xf9e2af),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
	};
	draw_grid(&mut painter, fonts, &view, None);
}

fn selected(options: &Options) -> impl Iterator<Item = &'static Workload> + '_ {
	WORKLOADS.iter().filter(|workload| {
		options.only.is_empty() || options.only.iter().any(|name| name == workload.name)
	})
}

/// `poopterm --benchmark [--mib N] [--size COLSxROWS] [--only a,b] [--read BYTES]
/// [--frame-every BYTES] [--repeat N] [--no-render] [--markdown] [--list]`
pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
	let options = options(args)?;
	let mut fonts = fonts();
	let (size, feed) = (options.size, options.feed);
	let table = report(&options, |stage, data| match stage {
		Stage::Parse => measure(data, size, feed, None),
		Stage::Render => measure(data, size, feed, Some(&mut fonts)),
	});
	print!("{}", table.render(options.style));
	Ok(())
}

/// `poopterm --bench-corpus NAME [--mib N | --kib N] [--size COLSxROWS]`: the bytes of one workload.
pub(crate) fn print_corpus(mut args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
	let name = args
		.next()
		.ok_or("missing workload name; try --benchmark --list")?;
	let workload = corpus::find(&name).ok_or_else(|| format!("unknown workload: {name}"))?;
	let options = options(args)?;
	let data = workload.build(options.bytes, options.size);
	std::io::stdout().lock().write_all(&data)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Parser and terminal only, every workload. Run with
	/// `cargo test --release -- --ignored --nocapture parser_throughput`;
	/// `poopterm --benchmark` reports the same plus rendering.
	#[test]
	#[ignore = "benchmark; run explicitly in release mode"]
	fn parser_throughput() {
		let size = Size {
			cols: 200,
			rows: 50,
		};
		for workload in WORKLOADS {
			let data = workload.build(32 << 20, size);
			println!(
				"parser {:<11} {:8.1} MB/s",
				workload.name,
				measure(&data, size, Feed::FLOOD, None)
			);
		}
	}

	/// Every workload parses and renders without panicking, at a small size.
	#[test]
	fn every_workload_runs_headless() {
		let mut fonts = fonts();
		let size = Size { cols: 40, rows: 12 };
		for workload in WORKLOADS {
			let data = workload.build(FRAME_EVERY + 1, size);
			assert!(
				measure(&data, size, Feed::FLOOD, Some(&mut fonts)) > 0.0,
				"{}",
				workload.name
			);
		}
	}

	fn parsed(args: &[&str]) -> Result<Options, String> {
		options(args.iter().map(|arg| arg.to_string())).map_err(|error| error.to_string())
	}

	#[test]
	fn floods_are_fed_in_pty_sized_reads_by_default() {
		let options = parsed(&[]).unwrap();
		assert_eq!(
			(options.feed.read, options.feed.frame_every),
			(READ, FRAME_EVERY)
		);
	}

	#[test]
	fn small_reads_and_frequent_frames_can_be_chosen() {
		let options = parsed(&["--read", "16", "--frame-every", "64"]).unwrap();
		assert_eq!((options.feed.read, options.feed.frame_every), (16, 64));
		// Zero would never advance; anything above a megabyte is one flood.
		let options = parsed(&["--read", "0", "--frame-every", "0"]).unwrap();
		assert_eq!((options.feed.read, options.feed.frame_every), (1, 1));
		let options = parsed(&["--read", "99999999"]).unwrap();
		assert_eq!(options.feed.read, 1 << 20);
	}

	#[test]
	fn three_timed_runs_by_default_and_as_many_as_asked() {
		assert_eq!(parsed(&[]).unwrap().repeat, 3);
		assert_eq!(parsed(&["--repeat", "7"]).unwrap().repeat, 7);
		// Zero runs would report nothing; fifty is already minutes per workload.
		assert_eq!(parsed(&["--repeat", "0"]).unwrap().repeat, 1);
		assert_eq!(parsed(&["--repeat", "1000"]).unwrap().repeat, 50);
		assert_eq!(
			parsed(&["--repeat"]).err().unwrap(),
			"--repeat needs a value"
		);
		assert!(parsed(&["--repeat", "-1"]).is_err());
	}

	#[test]
	fn text_by_default_and_markdown_when_asked() {
		assert_eq!(parsed(&[]).unwrap().style, Style::Text);
		assert_eq!(parsed(&["--markdown"]).unwrap().style, Style::Markdown);
		// Later options still apply after it; it takes no value.
		let options = parsed(&["--markdown", "--repeat", "2"]).unwrap();
		assert_eq!((options.style, options.repeat), (Style::Markdown, 2));
	}

	#[test]
	fn read_options_need_a_number() {
		assert_eq!(parsed(&["--read"]).err().unwrap(), "--read needs a value");
		assert!(parsed(&["--frame-every", "lots"]).is_err());
	}

	/// The cursor column at each frame drawn while `len` bytes arrive in
	/// reads of `read`.
	fn frames(len: usize, read: usize, frame_every: usize) -> Vec<usize> {
		let theme = Theme::from_kind(ThemeKind::Poop);
		let mut term = Term::new(30, 4, theme, "/".into());
		let mut drawn = Vec::new();
		let data = vec![b'x'; len];
		feed(
			&mut Parser::new(),
			&mut term,
			&data,
			Feed { read, frame_every },
			|term| {
				drawn.push(term.cx);
			},
		);
		drawn
	}

	#[test]
	fn frames_follow_whole_reads() {
		// Reads of 3 reach 5 bytes since the last frame at 6, 12 and 18.
		assert_eq!(frames(20, 3, 5), [6, 12, 18]);
		assert_eq!(frames(20, 20, 5), [20]);
		assert_eq!(frames(4, 3, 5), []);
		assert_eq!(frames(3, 1, 1), [1, 2, 3]);
	}

	#[test]
	fn replies_are_dropped_after_every_read() {
		let theme = Theme::from_kind(ThemeKind::Poop);
		let mut term = Term::new(30, 4, theme, "/".into());
		let feed_once = Feed {
			read: 4,
			frame_every: 4,
		};
		feed(
			&mut Parser::new(),
			&mut term,
			b"\x1b[6n\x1b[c",
			feed_once,
			|term| {
				assert!(term.responses.is_empty());
			},
		);
		assert!(term.responses.is_empty());
	}

	#[test]
	fn small_reads_leave_the_screen_a_flood_leaves() {
		let size = Size { cols: 30, rows: 8 };
		for workload in WORKLOADS
			.iter()
			.filter(|workload| workload.name != "kitty-rgba")
		{
			let data = workload.build(8 << 10, size);
			let screen = |read| {
				let theme = Theme::from_kind(ThemeKind::Poop);
				let mut term = Term::new(size.cols, size.rows, theme, "/".into());
				let mut parser = Parser::new();
				feed(
					&mut parser,
					&mut term,
					&data,
					Feed {
						read,
						frame_every: READ,
					},
					|_| {},
				);
				let rows: Vec<String> = (0..size.rows)
					.map(|y| term.grid[y].iter().map(|cell| cell.ch).collect())
					.collect();
				(rows, term.cx, term.cy)
			};
			assert_eq!(screen(7), screen(READ), "{}", workload.name);
		}
	}
}
