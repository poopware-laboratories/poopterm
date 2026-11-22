//! Hardening by brute force: random streams made of fragments of every
//! kind of sequence a program can send, fed in random pieces to terminals
//! of random sizes that are resized as they go. Nothing may panic (tests
//! run with overflow checks, so wrapping arithmetic counts), and the
//! cursor and grid must stay in bounds after every piece.
use crate::{
	bench::corpus::Rng,
	parser::Parser,
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

/// Numbers programs send as parameters: ordinary, edge and hostile.
const NUMBERS: [&str; 14] = [
	"",
	"0",
	"1",
	"2",
	"7",
	"9",
	"65535",
	"65536",
	"4294967295",
	"99999999999999999999",
	"-1",
	"1:2:3",
	"255",
	"1000",
];

fn number(rng: &mut Rng) -> String {
	rng.pick(&NUMBERS).to_string()
}

/// `count` random parameters joined by `;`.
fn params(rng: &mut Rng, count: usize) -> String {
	(0..count)
		.map(|_| number(rng))
		.collect::<Vec<_>>()
		.join(";")
}

/// Up to `most` random parameters.
fn params_of(rng: &mut Rng, most: usize) -> String {
	let count = rng.below(most);
	params(rng, count)
}

/// One random fragment: text, a control, or a sequence of some family.
fn fragment(rng: &mut Rng) -> Vec<u8> {
	let mut out = Vec::new();
	match rng.below(16) {
		0 => out.extend(b"hello world "),
		1 => out.extend("é漢字👍🏽‍🔥\u{301}\u{fe0f}🇫🇷".as_bytes()),
		2 => out.push(rng.below(32) as u8),
		3 => out.extend([0xc3, 0xff, 0xe2, 0x82, 0xf0, 0x9f]),
		4 => {
			// CSI with private markers, intermediates and finals of all kinds.
			let marker = *rng.pick(&["", "?", ">", "<", "=", "!"]);
			let middle = *rng.pick(&["", " ", "$", "\"", "'", "*", "#"]);
			let finals = b"@ABCDEFGHIJKLMPSTXZ`abcdefghijklmnpqrstuvxz{|}~";
			let final_byte = finals[rng.below(finals.len())] as char;
			let count = rng.below(20);
			out.extend(format!("\x1b[{marker}{}{middle}{final_byte}", params(rng, count)).as_bytes());
		}
		5 => {
			// DEC private modes, the many that change the screen's behavior.
			let modes = [
				1, 3, 5, 6, 7, 25, 47, 69, 1000, 1002, 1003, 1004, 1006, 1007, 1016, 1047, 1048, 1049,
				2004, 2026, 2027,
			];
			let action = *rng.pick(&["h", "l", "$p"]);
			out.extend(format!("\x1b[?{}{action}", rng.pick(&modes)).as_bytes());
		}
		6 => {
			let codes = [
				"0", "1", "2", "4", "7", "8", "9", "10", "11", "12", "52", "104", "110", "133", "1337",
				"5522", "7501", "99", "777", "22", "66", "176",
			];
			let payload: String = (0..rng.below(40))
				.map(|_| {
					*rng.pick(&[
						"a",
						";",
						"=",
						":",
						"?",
						"c",
						"Zm9v",
						"file:///etc",
						"A",
						"rgb:1/2/3",
					])
				})
				.collect();
			let end = *rng.pick(&["\x07", "\x1b\\", ""]);
			out.extend(format!("\x1b]{};{payload}{end}", rng.pick(&codes)).as_bytes());
		}
		7 => {
			// Sixel with hostile raster attributes and repeats.
			out.extend(
				format!(
					"\x1bP0;0;0q\"1;1;{};{}#1;2;100;0;0#1!{}~-$!{}@\x1b\\",
					number(rng),
					number(rng),
					number(rng),
					number(rng)
				)
				.as_bytes(),
			);
		}
		8 => {
			// Kitty graphics with random keys, payloads and chunking.
			let keys = [
				"a=T", "a=t", "a=p", "a=d", "a=f", "a=a", "a=c", "a=q", "f=24", "f=32", "f=100", "s=4",
				"v=4", "x=1", "c=2", "r=3", "m=1", "m=0", "i=1", "p=2", "U=1", "o=z", "t=d", "z=-1", "X=9",
				"q=2",
			];
			let control: Vec<&str> = (0..rng.below(6)).map(|_| *rng.pick(&keys)).collect();
			let payload = *rng.pick(&["", "AAAA", "/////w==", "iVBORw0KGgo=", "eJw=", "!!!"]);
			out.extend(format!("\x1b_G{};{payload}\x1b\\", control.join(",")).as_bytes());
		}
		9 => out.extend(
			format!(
				"\x1bP{}{}\x1b\\",
				rng.pick(&["$q", "+q", "$t", "=1s", "1$r"]),
				rng.pick(&["m", "r", "\"p", "544e", "636f6c6f7273", ""])
			)
			.as_bytes(),
		),
		10 => out.extend(
			rng
				.pick(&[
					"\x1b7", "\x1b8", "\x1bc", "\x1bD", "\x1bE", "\x1bH", "\x1bM", "\x1b#8", "\x1b(0",
					"\x1b(B", "\x1b)0", "\x0e", "\x0f", "\x1b=", "\x1b>", "\x1bN", "\x1bO", "\x1b%G",
				])
				.as_bytes(),
		),
		11 => out.extend(
			format!(
				"\x1b[{};{}r\x1b[{}S\x1b[{}T",
				number(rng),
				number(rng),
				number(rng),
				number(rng)
			)
			.as_bytes(),
		),
		12 => out.extend(
			format!(
				"\x1b[{};{}H\x1b[{}@\x1b[{}P\x1b[{}X\x1b[{}L\x1b[{}M",
				number(rng),
				number(rng),
				number(rng),
				number(rng),
				number(rng),
				number(rng),
				number(rng)
			)
			.as_bytes(),
		),
		13 => out.extend(format!("\x1b[{}m", params_of(rng, 12)).as_bytes()),
		14 => out.extend(
			format!(
				"\x1b]8;id={};{}\x1b\\link\x1b]8;;\x1b\\",
				number(rng),
				rng.pick(&["https://a.b", "file:///x", ""])
			)
			.as_bytes(),
		),
		_ => out.extend(
			format!(
				"\x1b[{};{};{};{}t",
				number(rng),
				number(rng),
				number(rng),
				number(rng)
			)
			.as_bytes(),
		),
	}
	out
}

/// Checks what must hold whatever was sent.
fn check(term: &Term, round: u64) {
	assert!(
		term.cx <= term.cols && term.cy < term.rows,
		"round {round}: cursor {},{} in {}x{}",
		term.cx,
		term.cy,
		term.cols,
		term.rows
	);
	for row in 0..term.scrollback.len() + term.rows {
		let cells = term
			.row_cells(row)
			.unwrap_or_else(|| panic!("round {round}: row {row} missing"));
		assert!(
			cells.len() >= term.cols || row < term.scrollback.len(),
			"round {round}: row {row} short"
		);
	}
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	assert_eq!(
		(frame.cols, frame.rows),
		(term.cols, term.rows),
		"round {round}"
	);
}

/// Feeds `rounds` random streams, from `seed`.
fn fuzz(seed: u64, rounds: u64) {
	let mut rng = Rng::new(seed);
	for round in 0..rounds {
		let (cols, rows) = (1 + rng.below(90), 1 + rng.below(30));
		let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
		(term.cell_width, term.cell_height) = (8, 16);
		let mut parser = Parser::new();
		for _ in 0..40 {
			let bytes: Vec<u8> = (0..1 + rng.below(6))
				.flat_map(|_| fragment(&mut rng))
				.collect();
			// Split anywhere: sequences arrive cut at any byte.
			let cut = rng.below(bytes.len() + 1);
			parser.advance(&mut term, &bytes[..cut]);
			parser.advance(&mut term, &bytes[cut..]);
			if rng.below(8) == 0 {
				term.resize(1 + rng.below(120), 1 + rng.below(40));
			}
			term.responses.clear();
			check(&term, round);
		}
	}
}

#[test]
fn random_streams_of_every_kind_of_sequence_never_break_the_terminal() {
	fuzz(0x5eed, 400);
}

#[test]
#[ignore = "long: cargo test --release -- --ignored fuzz_long"]
fn fuzz_long() {
	for seed in 0..64 {
		fuzz(seed, 2000);
	}
}
