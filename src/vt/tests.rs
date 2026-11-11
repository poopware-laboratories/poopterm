//! Differential tests: the parser must produce the same events as vte 0.15
//! for any byte stream, however it is split into reads.
// The tests call C, the kernel or CPU-dispatched copies directly.
#![allow(unsafe_code)]
use super::{Handler, Params, Parser, Text};
use crate::bench::corpus::Rng;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
	Print(String),
	Execute(u8),
	Csi(Vec<Vec<u16>>, Vec<u8>, bool, u8),
	Esc(Vec<u8>, bool, u8),
	Osc(Vec<Vec<u8>>, bool),
	Hook(Vec<Vec<u16>>, Vec<u8>, bool, u8),
	Put(Vec<u8>),
	Unhook,
	Apc(Vec<u8>, bool),
}

/// Records events, merging adjacent text and payload so that how either
/// parser splits them does not matter.
#[derive(Default)]
struct Recorder {
	events: Vec<Event>,
	apc: Option<Vec<u8>>,
}

impl Recorder {
	fn text(&mut self, text: &str) {
		if let Some(Event::Print(last)) = self.events.last_mut() {
			last.push_str(text);
		} else {
			self.events.push(Event::Print(text.to_owned()));
		}
	}

	fn payload(&mut self, bytes: &[u8]) {
		if let Some(Event::Put(last)) = self.events.last_mut() {
			last.extend_from_slice(bytes);
		} else {
			self.events.push(Event::Put(bytes.to_vec()));
		}
	}

	/// Events vte also reports: everything but APC strings.
	fn without_apc(&self) -> Vec<Event> {
		let mut recorder = Recorder::default();
		for event in &self.events {
			match event {
				Event::Apc(..) => {}
				Event::Print(text) => recorder.text(text),
				Event::Put(bytes) => recorder.payload(bytes),
				other => recorder.events.push(other.clone()),
			}
		}
		recorder.events
	}
}

fn groups(params: impl Iterator<Item = impl AsRef<[u16]>>) -> Vec<Vec<u16>> {
	params.map(|group| group.as_ref().to_vec()).collect()
}

impl vte::Perform for Recorder {
	fn print(&mut self, c: char) {
		self.text(c.encode_utf8(&mut [0; 4]));
	}
	fn execute(&mut self, byte: u8) {
		self.events.push(Event::Execute(byte));
	}
	fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
		let event = Event::Hook(
			groups(params.iter()),
			intermediates.to_vec(),
			ignore,
			action as u8,
		);
		self.events.push(event);
	}
	fn put(&mut self, byte: u8) {
		self.payload(&[byte]);
	}
	fn unhook(&mut self) {
		self.events.push(Event::Unhook);
	}
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.events.push(Event::Osc(
			params.iter().map(|p| p.to_vec()).collect(),
			bell,
		));
	}
	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		let event = Event::Csi(
			groups(params.iter()),
			intermediates.to_vec(),
			ignore,
			action as u8,
		);
		self.events.push(event);
	}
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self
			.events
			.push(Event::Esc(intermediates.to_vec(), ignore, byte));
	}
}

impl Handler for Recorder {
	fn print_ascii(&mut self, text: &[u8]) {
		assert!(
			text.iter().all(|byte| (0x20..0x7f).contains(byte)),
			"{text:?}"
		);
		self.text(std::str::from_utf8(text).unwrap());
	}
	fn print(&mut self, text: Text<'_>) {
		let text = std::str::from_utf8(text.as_bytes()).expect("printed text is UTF-8");
		assert!(!text.is_empty());
		assert!(
			!text
				.chars()
				.any(|c| c < ' ' || ('\u{80}'..='\u{9f}').contains(&c)),
			"{text:?}"
		);
		self.text(text);
	}
	fn execute(&mut self, byte: u8) {
		self.events.push(Event::Execute(byte));
	}
	fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.events.push(Event::Csi(
			groups(params.iter()),
			intermediates.to_vec(),
			ignore,
			action,
		));
	}
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self
			.events
			.push(Event::Esc(intermediates.to_vec(), ignore, byte));
	}
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.events.push(Event::Osc(
			params.iter().map(|p| p.to_vec()).collect(),
			bell,
		));
	}
	fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
		self.events.push(Event::Hook(
			groups(params.iter()),
			intermediates.to_vec(),
			ignore,
			action,
		));
	}
	fn put(&mut self, bytes: &[u8]) {
		assert!(!bytes.is_empty());
		self.payload(bytes);
	}
	fn unhook(&mut self) {
		self.events.push(Event::Unhook);
	}
	fn apc_start(&mut self) {
		assert!(self.apc.replace(Vec::new()).is_none(), "nested APC");
	}
	fn apc_put(&mut self, bytes: &[u8]) {
		self
			.apc
			.as_mut()
			.expect("APC started")
			.extend_from_slice(bytes);
	}
	fn apc_end(&mut self, complete: bool) {
		let apc = self.apc.take().expect("APC started");
		self.events.push(Event::Apc(apc, complete));
	}
}

fn reference(stream: &[u8]) -> Vec<Event> {
	let mut recorder = Recorder::default();
	vte::Parser::new().advance(&mut recorder, stream);
	recorder.events
}

/// Feeds `stream` in reads that end at `splits`.
fn ours(stream: &[u8], splits: &[usize]) -> Recorder {
	let mut recorder = Recorder::default();
	let mut parser = Parser::new();
	let mut start = 0;
	for &end in splits.iter().chain([&stream.len()]) {
		parser.advance(&mut recorder, &stream[start..end]);
		start = end;
	}
	recorder
}

fn agrees(stream: &[u8], splits: &[usize]) -> bool {
	ours(stream, splits).without_apc() == reference(stream)
}

/// Panics with the smallest stream (removing bytes one at a time) that still
/// disagrees, read whole or byte by byte as `bytewise` says.
fn check_with(stream: &[u8], splits: &[usize], bytewise: bool) {
	if agrees(stream, splits) {
		return;
	}
	let all = |len: usize| {
		if bytewise {
			(1..len).collect()
		} else {
			Vec::new()
		}
	};
	let mut stream = stream.to_vec();
	if bytewise || splits.is_empty() {
		let mut index = 0;
		while index < stream.len() {
			let mut smaller = stream.clone();
			smaller.remove(index);
			if !agrees(&smaller, &all(smaller.len())) {
				stream = smaller;
			} else {
				index += 1;
			}
		}
	}
	let splits = if bytewise || splits.is_empty() {
		all(stream.len())
	} else {
		splits.to_vec()
	};
	let actual = ours(&stream, &splits).without_apc();
	let expected = reference(&stream);
	panic!(
		"stream {:?}\nsplits {splits:?}\nours {actual:#?}\nvte  {expected:#?}",
		String::from_utf8_lossy(&stream).escape_debug().to_string() + &format!(" {stream:x?}")
	);
}

fn check(stream: &[u8], splits: &[usize]) {
	check_with(stream, splits, false);
}

/// Pieces that exercise every state and transition, joined at random.
fn fragment(rng: &mut Rng, out: &mut Vec<u8>) {
	const PIECES: &[&[u8]] = &[
		b"hello world ",
		b"\x1b[?9h",
		b"\x1b[?1005;1015h",
		b"\x1b[?1015$p",
		b"\x1b[",
		b"\x1b]",
		b"\x1bP",
		b"\x1b_",
		b"\x1bX",
		b"\x1b^",
		b"\x1b\\",
		b"\x1b(B",
		b"\x1b#8",
		b"\x1b !#M",
		b"\x1b7",
		b"\x1b[38:2::10:20:30m",
		b"\x1b[?2026h",
		b"\x1b[>4;2m",
		b"\x1b[ q",
		b"\x1b[1;2;3$|",
		b"\x1b[?$p",
		b"\x1b]0;title\x07",
		b"\x1b]8;;http://x\x1b\\",
		b"\x1b]52;c;aGk=\x1b\\",
		b"\x1bP0;1;0q#0;2;0;0;0!10~-\x1b\\",
		b"\x1bP$q\"p\x1b\\",
		b"\x1b_Ga=T,f=32;AAAA\x1b\\",
		b"\xc3\xa9",
		b"\xe2\x94\x80",
		b"\xf0\x9f\x98\x80",
		b"\xe7\x8c\xab",
		b"\xc2\x85",
		b"\xc2\x9b",
		b"\xed\xa0\x80",
		b"\xc0\xaf",
		b"\xf4\x90\x80\x80",
		// The edges of each encoded length, of C1 and of the surrogates.
		b"\xc2\x9f",
		b"\xc2\xa0",
		b"\xdf\xbf",
		b"\xe0\xa0\x80",
		b"\xed\x9f\xbf",
		b"\xee\x80\x80",
		b"\xef\xbf\xbf",
		b"\xf0\x90\x80\x80",
		b"\xf4\x8f\xbf\xbf",
	];
	const BYTES: &[u8] = &[
		0x00, 0x07, 0x08, 0x0a, 0x0d, 0x18, 0x1a, 0x1b, 0x1c, 0x1f, b' ', b'0', b'9', b':', b';', b'<',
		b'?', b'!', b'$', b'@', b'm', b'~', b'\\', 0x7f, 0x80, 0x90, 0x9b, 0x9c, 0x9d, 0xa0, 0xbf,
		0xc2, 0xe2, 0xf0, 0xf5, 0xff,
	];
	match rng.below(10) {
		0..=4 => out.extend_from_slice(rng.pick(PIECES)),
		5..=7 => out.push(*rng.pick(BYTES)),
		8 => out.extend((0..rng.below(40)).map(|_| b'0' + rng.below(10) as u8)),
		_ => {
			// Long parameter lists overflow the parameter storage.
			for _ in 0..rng.below(40) {
				out.extend_from_slice(rng.pick(&[&b"1;"[..], b"22:", b"333", b";", b":"]));
			}
		}
	}
}

#[test]
fn matches_vte_on_random_streams_split_at_random() {
	let mut rng = Rng::new(0x5eed);
	for _ in 0..20_000 {
		let mut stream = Vec::new();
		for _ in 0..1 + rng.below(24) {
			fragment(&mut rng, &mut stream);
		}
		let mut splits: Vec<usize> = (0..rng.below(4))
			.map(|_| rng.below(stream.len() + 1))
			.collect();
		splits.sort_unstable();
		check(&stream, &[]);
		check(&stream, &splits);
	}
}

#[test]
fn matches_vte_when_every_byte_is_a_separate_read() {
	let mut rng = Rng::new(7);
	for _ in 0..2_000 {
		let mut stream = Vec::new();
		for _ in 0..1 + rng.below(12) {
			fragment(&mut rng, &mut stream);
		}
		let splits: Vec<usize> = (1..stream.len()).collect();
		check_with(&stream, &splits, true);
	}
}

#[test]
fn matches_vte_on_long_runs_across_vector_boundaries() {
	let mut stream = Vec::new();
	for len in [31, 32, 33, 63, 64, 65, 200] {
		stream.extend(std::iter::repeat_n(b'a', len));
		stream.extend_from_slice("é猫".as_bytes());
		stream.extend(std::iter::repeat_n(b'b', len));
		stream.extend_from_slice(b"\x1b]2;");
		stream.extend(std::iter::repeat_n(b't', len));
		stream.extend_from_slice(b"\x07\x1bPq");
		stream.extend(std::iter::repeat_n(b'~', len));
		stream.extend_from_slice(b"\x1b\\");
	}
	check(&stream, &[]);
	check(&stream, &[1, 33, 100, 500]);
}

/// Long Unicode runs take the SIMD validation path; a defect anywhere in
/// them must fall back to the same events as vte.
#[test]
fn matches_vte_with_defects_anywhere_in_long_unicode_runs() {
	let run = "Größe ελληνικά кириллица 日本語 😀 ".repeat(3);
	let defects: [&[u8]; 9] = [
		b"\xc2\x85",
		b"\xc2\x9b",
		b"\xed\xa0\x80",
		b"\xc0\xaf",
		b"\xf4\x90\x80\x80",
		b"\xe2\x94",
		b"\xff",
		b"\x80",
		b"\x1b[31m",
	];
	for defect in defects {
		for at in (0..=run.len()).filter(|&at| run.is_char_boundary(at)) {
			let mut stream = run.as_bytes()[..at].to_vec();
			stream.extend_from_slice(defect);
			stream.extend_from_slice(&run.as_bytes()[at..]);
			check(&stream, &[]);
			check(&stream, &[at, at + 1]);
			// A read ending inside the defect, then everything else.
			check(&stream, &[at + defect.len() / 2]);
		}
	}
}

#[test]
fn a_character_cut_by_a_read_inside_a_long_run_prints_once() {
	let run = "αβγδ€😀".repeat(12);
	for split in 0..=run.len() {
		let recorder = ours(run.as_bytes(), &[split]);
		check(run.as_bytes(), &[split]);
		let printed: String = recorder
			.events
			.iter()
			.filter_map(|event| match event {
				Event::Print(text) => Some(text.as_str()),
				_ => None,
			})
			.collect();
		assert_eq!(printed, run, "split at {split}");
	}
}

/// Text and controls in the order the parser reported them.
fn printed_and_executed(recorder: &Recorder) -> (String, Vec<u8>) {
	let (mut text, mut controls) = (String::new(), Vec::new());
	for event in &recorder.events {
		match event {
			Event::Print(printed) => text.push_str(printed),
			Event::Execute(byte) => controls.push(*byte),
			other => panic!("unexpected {other:?}"),
		}
	}
	(text, controls)
}

#[test]
fn every_encoded_c1_control_executes_at_every_block_edge() {
	let run = "日本語 ελληνικά ".repeat(6);
	let edges = (0..=run.len()).filter(|&at| run.is_char_boundary(at) && at % 32 < 4);
	for at in edges {
		for control in 0x80..=0x9fu32 {
			let encoded = char::from_u32(control).unwrap().to_string();
			let stream = [&run[..at], &encoded, &run[at..]].concat();
			check(stream.as_bytes(), &[]);
			let (text, controls) = printed_and_executed(&ours(stream.as_bytes(), &[]));
			assert_eq!(text, run, "U+{control:04X} at {at}");
			assert_eq!(controls, [control as u8]);
		}
	}
}

#[test]
fn every_printable_scalar_survives_reads_of_any_size() {
	let text: String = (0x20..=0x10ffff)
		.filter_map(char::from_u32)
		.filter(|ch| !('\u{7f}'..='\u{9f}').contains(ch))
		.collect();
	for read in [4093, 65_536] {
		let splits: Vec<_> = (read..text.len()).step_by(read).collect();
		let (printed, controls) = printed_and_executed(&ours(text.as_bytes(), &splits));
		assert!(printed == text, "reads of {read}");
		assert!(controls.is_empty());
	}
}

#[test]
fn a_defect_after_a_long_valid_prefix_prints_one_replacement() {
	let prefix = "日本語".repeat(20);
	for (defect, expected) in [
		(&b"\xff"[..], "\u{fffd}"),
		(b"\xed\xa0\xa0", "\u{fffd}\u{fffd}\u{fffd}"),
		(b"\xe6\x97", "\u{fffd}"),
		(b"\xf0\x9f\x98", "\u{fffd}"),
	] {
		let stream = [prefix.as_bytes(), defect, b"a"].concat();
		check(&stream, &[]);
		let (text, controls) = printed_and_executed(&ours(&stream, &[]));
		assert_eq!(text, format!("{prefix}{expected}a"), "{defect:x?}");
		assert!(controls.is_empty());
	}
}

#[test]
fn delivers_apc_strings_in_pieces() {
	let stream = b"a\x1b_Gi=1;AAAA\x1b\\b\x1b_x\x18c";
	for split in 0..stream.len() {
		let events = ours(stream, &[split]).events;
		assert_eq!(
			events,
			[
				Event::Print("a".into()),
				Event::Apc(b"Gi=1;AAAA".to_vec(), true),
				Event::Esc(vec![], false, b'\\'),
				Event::Print("b".into()),
				Event::Apc(b"x".to_vec(), false),
				Event::Execute(0x18),
				Event::Print("c".into()),
			],
			"split at {split}"
		);
	}
}

#[test]
fn oversized_osc_strings_are_dropped() {
	let mut stream = b"\x1b]0;".to_vec();
	stream.resize(super::MAX_OSC_BYTES + 10, b'x');
	stream.extend_from_slice(b"\x07\x1b]0;ok\x07");
	let events = ours(&stream, &[4096]).events;
	assert_eq!(
		events,
		[Event::Osc(vec![b"0".to_vec(), b"ok".to_vec()], true)]
	);
}

/// The x86-64-v3 variant must report exactly the baseline's events,
/// APC strings included, however the stream is split.
#[cfg(target_arch = "x86_64")]
#[test]
fn the_v3_variant_reports_the_same_events_as_the_baseline() {
	if !crate::cpu::has_v3() {
		return;
	}
	let mut rng = Rng::new(0x3);
	for _ in 0..5_000 {
		let mut stream = Vec::new();
		for _ in 0..1 + rng.below(24) {
			fragment(&mut rng, &mut stream);
		}
		let mut cuts: Vec<usize> = (0..rng.below(4))
			.map(|_| rng.below(stream.len() + 1))
			.collect();
		cuts.sort_unstable();
		cuts.push(stream.len());
		let (mut baseline, mut v3) = (Recorder::default(), Recorder::default());
		let (mut baseline_parser, mut v3_parser) = (Parser::default(), Parser::default());
		let mut start = 0;
		for &end in &cuts {
			baseline_parser.advance_with(&mut baseline, &stream[start..end]);
			// SAFETY: checked above.
			unsafe { v3_parser.advance_v3(&mut v3, &stream[start..end]) };
			start = end;
		}
		assert_eq!(v3.events, baseline.events, "cut at {cuts:?}: {stream:?}");
	}
}

mod malformed;
