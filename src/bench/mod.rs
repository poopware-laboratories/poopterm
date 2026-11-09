//! Benchmarks over a corpus of real-world workloads.
//!
//! `poopterm --benchmark` feeds every workload through the parser and
//! terminal, with and without rendering frames, several times, and prints
//! the median MB/s with its spread. It needs no window, so it is also the
//! training run for profile-guided optimization. Other stages send the same
//! bytes through a real PTY, Poopterm's own PTY pipeline, or (`--terminal`)
//! whatever terminal it runs in, so every terminal is measured on identical
//! input. `poopterm --bench-corpus NAME` prints a workload's bytes.
pub(crate) mod corpus;
pub(crate) mod images;
pub(crate) mod mark;
pub(crate) mod memory;
pub(crate) mod pty;
pub(crate) mod replies;
pub(crate) mod report;
pub(crate) mod stats;
pub(crate) mod table;
pub(crate) mod terminal;

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
use std::{error::Error, fs::File, io::Write, time::Instant};
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
	/// Whether `--size` was given; otherwise `--terminal` uses the window's.
	sized: bool,
	only: Vec<String>,
	stages: Vec<Stage>,
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
		sized: false,
		only: Vec::new(),
		stages: report::HEADLESS.to_vec(),
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
				options.size = Size::bounded(cols.parse()?, rows.parse()?);
				options.sized = true;
			}
			"--read" => options.feed.read = value()?.parse::<usize>()?.clamp(1, 1 << 20),
			"--frame-every" => options.feed.frame_every = value()?.parse::<usize>()?.clamp(1, 1 << 30),
			"--only" => options.only.extend(value()?.split(',').map(str::to_owned)),
			"--no-render" => options.stages.retain(|&stage| stage != Stage::Render),
			"--stages" => options.stages = report::stages(&value()?)?,
			"--terminal" => options.stages = report::stages("pty,pty-raw,terminal,terminal-raw")?,
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
	if options.stages.is_empty() {
		return Err("no stages left to run".into());
	}
	Ok(options)
}

fn fonts() -> FontSet {
	let font =
		crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).expect("bundled font is valid");
	FontSet::new(font, Vec::new())
}

/// A terminal of `size` whose history is full, as after real use: costs
/// that grow with history (copying it, say) show up only when there is some.
pub(crate) fn primed(size: Size) -> Term {
	let theme = Theme::from_kind(ThemeKind::Poop);
	let mut term = Term::new(size.cols, size.rows, theme, "/".into());
	(term.cell_width, term.cell_height) = CELL;
	let line = format!("{}\r\n", "x".repeat(size.cols / 2));
	let lines = term.scrollback_limit + size.rows;
	Parser::new().advance(&mut term, line.repeat(lines).as_bytes());
	term.responses.clear();
	term
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
	let mut term = primed(size);
	let mut parser = Parser::new();
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
		dim: None,
	};
	draw_grid(&mut painter, fonts, &view, None);
}

fn selected(options: &Options) -> impl Iterator<Item = &'static Workload> + '_ {
	WORKLOADS.iter().filter(|workload| {
		options.only.is_empty() || options.only.iter().any(|name| name == workload.name)
	})
}

/// `poopterm --benchmark [--mib N] [--size COLSxROWS] [--only a,b] [--read BYTES]
/// [--frame-every BYTES] [--repeat N] [--stages LIST] [--terminal] [--no-render]
/// [--markdown] [--list]`
pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), Box<dyn Error>> {
	let options = options(args)?;
	let style = options.style;
	let in_terminal = |stage: &Stage| matches!(stage, Stage::Terminal(_));
	let mut tty = None;
	if options.stages.iter().any(in_terminal) {
		let file = File::options().read(true).write(true).open("/dev/tty");
		let file = file.map_err(|error| format!("--terminal needs a terminal to run in: {error}"))?;
		tty = Some((file, memory::terminal_pid(std::process::id(), memory::stat)));
	}
	print!("{}", benchmark(options, tty)?.render(style));
	Ok(())
}

/// The report for `options`, measuring terminal stages in `tty`, whose
/// emulator is process `pid` when known. A terminal sets the size unless
/// `--size` did, its XTVERSION answer names it in the first note, and its
/// memory gets a note when its process is known. It is given back before
/// this returns.
fn benchmark(
	mut options: Options,
	tty: Option<(File, Option<u32>)>,
) -> Result<table::Table, Box<dyn Error>> {
	let (tty, pid) = tty.unzip();
	let pid = pid.flatten();
	let before = pid.and_then(memory::resident_kib);
	let mut terminal = tty.map(terminal::Terminal::open).transpose()?;
	let mut name = None;
	if let Some(terminal) = &mut terminal {
		name = replies::xtversion(&terminal.ask(b"\x1b[>q")?);
		if !options.sized {
			options.size = terminal.size()?;
		}
	}
	let mut fonts = fonts();
	let (size, feed) = (options.size, options.feed);
	let mut table = report(&options, |stage, data| match stage {
		Stage::Parse => Ok(measure(data, size, feed, None)),
		Stage::Render => Ok(measure(data, size, feed, Some(&mut fonts))),
		Stage::Pty(mode) => pty::rate(data, size, mode, false),
		Stage::Pipeline(mode) => pty::rate(data, size, mode, true),
		Stage::Terminal(mode) => (terminal.as_mut())
			.ok_or("terminal stages need a terminal")?
			.rate(data, mode),
	})?;
	if terminal.take().is_some() {
		let name = name
			.as_deref()
			.unwrap_or("a terminal that did not say its name");
		table.notes.insert(0, format!("measured in {name}"));
	}
	if let Some(pid) = pid {
		let (after, peak) = (memory::resident_kib(pid), memory::peak(pid));
		table.notes.push(memory::note(pid, before, after, peak));
	}
	Ok(table)
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
	fn a_primed_terminal_has_a_full_history_and_nothing_to_answer() {
		let term = primed(Size { cols: 30, rows: 5 });
		assert_eq!(term.scrollback.len(), term.scrollback_limit);
		assert!(term.scrollback_limit > 0);
		assert_eq!((term.cell_width, term.cell_height), CELL);
		assert!(term.responses.is_empty());
		// Half-width lines, and the cursor home on the empty last one.
		let top: String = term.grid[0].iter().map(|cell| cell.ch).collect();
		assert_eq!(top.trim_end(), "x".repeat(15));
		assert_eq!((term.cx, term.cy), (0, 4));
	}

	#[test]
	fn both_headless_stages_run_by_default() {
		assert_eq!(parsed(&[]).unwrap().stages, [Stage::Parse, Stage::Render]);
		assert_eq!(parsed(&["--no-render"]).unwrap().stages, [Stage::Parse]);
	}

	#[test]
	fn stages_are_chosen_by_name() {
		let options = parsed(&["--stages", "pipeline-raw,pty"]).unwrap();
		assert_eq!(
			options.stages,
			[
				Stage::Pipeline(pty::Mode::Raw),
				Stage::Pty(pty::Mode::Cooked)
			]
		);
	}

	#[test]
	fn the_later_of_no_render_and_stages_wins() {
		// --no-render takes rendering out of whatever was chosen so far.
		let dropped = parsed(&["--stages", "render,pty-raw", "--no-render"]).unwrap();
		assert_eq!(dropped.stages, [Stage::Pty(pty::Mode::Raw)]);
		let chosen = parsed(&["--no-render", "--stages", "render"]).unwrap();
		assert_eq!(chosen.stages, [Stage::Render]);
	}

	#[test]
	fn no_render_after_all_drops_only_rendering() {
		let stages = parsed(&["--stages", "all", "--no-render"]).unwrap().stages;
		assert_eq!(stages.len(), 5, "{stages:?}");
		assert!(!stages.contains(&Stage::Render));
		assert_eq!(stages[0], Stage::Parse);
	}

	#[test]
	fn dropping_the_only_stage_is_an_error() {
		let error = parsed(&["--stages", "render", "--no-render"])
			.err()
			.unwrap();
		assert_eq!(error, "no stages left to run");
		assert!(parsed(&["--stages", "render,pty", "--no-render"]).is_ok());
	}

	#[test]
	fn terminal_runs_the_terminal_and_its_ceiling_in_both_modes() {
		let options = parsed(&["--terminal"]).unwrap();
		use {Stage::*, pty::Mode::*};
		let expected = [Pty(Cooked), Pty(Raw), Terminal(Cooked), Terminal(Raw)];
		assert_eq!(options.stages, expected);
		assert!(!options.sized);
		assert!(parsed(&["--size", "80x24", "--terminal"]).unwrap().sized);
	}

	#[test]
	fn a_size_given_is_bounded_and_must_be_cols_by_rows() {
		let size = |value| parsed(&["--size", value]).map(|options| options.size);
		assert_eq!(size("0x0"), Ok(Size { cols: 2, rows: 2 }));
		assert_eq!(
			size("5000x40"),
			Ok(Size {
				cols: 1000,
				rows: 40
			})
		);
		assert_eq!(size("80"), Err("--size is COLSxROWS".into()));
		assert!(size("80x").is_err());
		assert!(size("-1x24").is_err());
	}

	#[test]
	fn the_later_of_terminal_and_stages_wins() {
		let options = parsed(&["--terminal", "--stages", "parse"]).unwrap();
		assert_eq!(options.stages, [Stage::Parse]);
		let options = parsed(&["--stages", "parse", "--terminal"]).unwrap();
		assert!(
			!options.stages.contains(&Stage::Parse),
			"{:?}",
			options.stages
		);
	}

	/// A quick benchmark of `log` through Poopterm's own terminal on a PTY
	/// of 93×27, with `more` options.
	fn in_a_terminal(more: &[&str]) -> (table::Table, terminal::tests::Emulator) {
		let pid = Some(std::process::id());
		let (table, emulator) = in_one_answering(terminal::tests::Answers::Whole, pid, more);
		(table.unwrap(), emulator)
	}

	/// [`in_a_terminal`], with the terminal answering as `answers` says and
	/// its process `pid`.
	fn in_one_answering(
		answers: terminal::tests::Answers,
		pid: Option<u32>,
		more: &[&str],
	) -> (Result<table::Table, String>, terminal::tests::Emulator) {
		let (tty, emulator) = terminal::tests::emulator(93, 27, answers);
		let mut args = vec![
			"--terminal",
			"--only",
			"log",
			"--kib",
			"16",
			"--repeat",
			"1",
		];
		args.extend(more);
		let table = benchmark(parsed(&args).unwrap(), Some((tty, pid)));
		(table.map_err(|error| error.to_string()), emulator)
	}

	#[test]
	fn a_terminal_that_does_not_say_its_name_is_still_measured() {
		let (table, _) = in_one_answering(terminal::tests::Answers::Anonymous, None, &[]);
		let notes = table.unwrap().notes;
		assert_eq!(notes[0], "measured in a terminal that did not say its name");
		// Its process is unknown, so its memory goes unnoted.
		assert!(
			notes.iter().all(|note| !note.starts_with("memory")),
			"{notes:?}"
		);
	}

	#[test]
	fn ctrl_c_stops_the_benchmark_and_gives_the_terminal_back() {
		let (tty, emulator) = terminal::tests::emulator(93, 27, terminal::tests::Answers::Never);
		let watch = tty.try_clone().unwrap();
		let before = terminal::tests::settings(&watch);
		// Pressed once the run has the terminal raw: earlier, the line
		// discipline would take it as a signal.
		let user = std::thread::spawn(move || {
			while emulator.seen().is_empty() {
				std::thread::sleep(std::time::Duration::from_millis(5));
			}
			emulator.type_in(&[0x03]);
			emulator
		});
		let options = parsed(&["--terminal", "--only", "log", "--kib", "4"]).unwrap();
		let error = benchmark(options, Some((tty, None))).err().unwrap();
		assert_eq!(error.to_string(), "interrupted");
		assert_eq!(terminal::tests::settings(&watch), before);
		let user = user.join().unwrap();
		assert!(user.seen_ending(terminal::CLEAR).ends_with(terminal::CLEAR));
	}

	#[test]
	fn a_terminal_run_is_sized_and_named_by_the_terminal() {
		let (table, emulator) = in_a_terminal(&[]);
		assert!(table.title.contains(", 93x27, 16 KiB"), "{}", table.title);
		let version = env!("CARGO_PKG_VERSION");
		assert_eq!(table.notes[0], format!("measured in poopterm {version}"));
		assert_eq!(
			table.columns,
			["pty", "pty raw", "terminal", "terminal raw"]
		);
		let cells = &table.rows[0].cells;
		assert!(
			cells
				.iter()
				.all(|cell| cell.is_some_and(|spread| spread.median > 0.0))
		);
		// Every workload byte reached the terminal, the warm-up's too, and
		// the terminal was given back cleared.
		let seen = emulator.seen_ending(terminal::CLEAR);
		let log = corpus::find("log")
			.unwrap()
			.build(16 << 10, Size { cols: 93, rows: 27 });
		assert!(seen.len() > 2 * log.len(), "{}", seen.len());
		assert!(seen.ends_with(terminal::CLEAR));
		// This process stands in for the emulator's, whose memory is noted.
		let memory = table.notes.last().unwrap();
		let pid = std::process::id();
		assert!(
			memory.starts_with(&format!("memory of pid {pid}: ")),
			"{memory}"
		);
		assert!(!memory.contains("- MiB at the end"), "{memory}");
		assert!(!memory.contains("- MiB at start"), "{memory}");
	}

	#[test]
	fn a_size_given_wins_over_the_terminals() {
		let (table, _) = in_a_terminal(&["--size", "40x12"]);
		assert!(table.title.contains(", 40x12, "), "{}", table.title);
	}

	#[test]
	fn without_terminal_stages_no_terminal_is_needed_or_named() {
		let options = parsed(&["--only", "log", "--kib", "4", "--repeat", "1"]).unwrap();
		let table = benchmark(options, None).unwrap();
		assert!(
			table
				.notes
				.iter()
				.all(|note| !note.starts_with("measured in"))
		);
		let options = parsed(&["--stages", "terminal", "--only", "log", "--kib", "4"]).unwrap();
		let error = benchmark(options, None).err().unwrap().to_string();
		assert_eq!(error, "terminal stages need a terminal");
	}

	#[test]
	fn an_unknown_stage_is_an_error() {
		let error = parsed(&["--stages", "parse,gpu"]).err().unwrap();
		assert!(error.starts_with("unknown stage: gpu; "), "{error}");
		assert_eq!(
			parsed(&["--stages"]).err().unwrap(),
			"--stages needs a value"
		);
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
