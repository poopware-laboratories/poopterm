//! Poopterm's own escape-sequence tokenizer: the DEC/ECMA-48 state machine,
//! with SIMD scans for text and string payloads.
//!
//! [`Parser::advance`] turns bytes into [`Handler`] calls. Text, OSC strings,
//! DCS payloads and APC strings are handed over in bulk slices instead of
//! byte by byte. Sequence semantics match vte 0.15 (checked by the
//! differential tests in `tests.rs`), except that APC strings are delivered
//! (vte drops them) and characters split across reads are kept intact.
mod params;
mod sequence;
mod simd;
mod strings;
#[cfg(test)]
mod tests;
mod text;
mod utf8;
#[cfg(test)]
mod utf8_tests;

pub(crate) use params::Params;
pub(crate) use text::Text;

/// What a tokenized stream does. Text arrives in runs; a run never contains
/// C0 controls, and [`Handler::print_ascii`] runs are `0x20..=0x7e` only.
pub(crate) trait Handler {
	fn print_ascii(&mut self, text: &[u8]) {
		if let Some(text) = Text::new(text) {
			self.print(text);
		}
	}
	/// Text that may contain DEL and any character except C0 and C1 controls.
	fn print(&mut self, text: Text<'_>);
	/// A C0 control, or a C1 control (`0x80..=0x9f`) from ground state.
	fn execute(&mut self, byte: u8);
	fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);
	/// The `;`-separated fields of an OSC string; `bell` if BEL ended it.
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool);
	/// Starts a DCS payload; [`Handler::put`] slices follow until `unhook`.
	fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
	fn put(&mut self, bytes: &[u8]);
	fn unhook(&mut self);
	/// An APC string (`ESC _ … ESC \`), such as a kitty graphics command.
	fn apc_start(&mut self) {}
	fn apc_put(&mut self, _bytes: &[u8]) {}
	/// `complete` is false when CAN or SUB cancelled the string.
	fn apc_end(&mut self, _complete: bool) {}
}

/// Intermediates kept per sequence; more set the ignore flag.
const MAX_INTERMEDIATES: usize = 2;
/// OSC fields per string; whatever follows the last one's `;` is dropped.
const MAX_OSC_PARAMS: usize = 16;
/// Larger OSC strings are discarded instead of dispatched. OSC 52 copies are
/// the largest legitimate use and have their own, smaller limit.
const MAX_OSC_BYTES: usize = 4 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
	Ground,
	Escape,
	EscapeIntermediate,
	CsiEntry,
	CsiParam,
	CsiIntermediate,
	CsiIgnore,
	DcsEntry,
	DcsParam,
	DcsIntermediate,
	DcsPassthrough,
	DcsIgnore,
	Osc,
	Apc,
	/// SOS and PM: consumed and ignored.
	IgnoredString,
}

/// Stateful tokenizer for one stream. Sequences and UTF-8 characters split
/// across [`Parser::advance`] calls are completed by later calls.
pub(crate) struct Parser {
	state: State,
	params: Params,
	/// The parameter whose digits are being read.
	param: u16,
	intermediates: [u8; MAX_INTERMEDIATES],
	intermediate_len: usize,
	ignore: bool,
	osc: Vec<u8>,
	/// End of each finished OSC field in `osc`.
	osc_ends: [usize; MAX_OSC_PARAMS],
	osc_fields: usize,
	osc_overflow: bool,
	/// An incomplete UTF-8 character from the end of the previous read.
	partial: [u8; 4],
	partial_len: usize,
}

impl Default for Parser {
	fn default() -> Self {
		Self::new()
	}
}

impl Parser {
	pub(crate) fn new() -> Self {
		Self {
			state: State::Ground,
			params: Params::default(),
			param: 0,
			intermediates: [0; MAX_INTERMEDIATES],
			intermediate_len: 0,
			ignore: false,
			osc: Vec::new(),
			osc_ends: [0; MAX_OSC_PARAMS],
			osc_fields: 0,
			osc_overflow: false,
			partial: [0; 4],
			partial_len: 0,
		}
	}

	/// Feeds one read of the stream, through the x86-64-v3 variant when the
	/// CPU has it. The whole state machine and the handler's sequence path
	/// are inlined into each variant, so they share one dispatch per read.
	pub(crate) fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
		#[cfg(target_arch = "x86_64")]
		if crate::cpu::has_v3() {
			// SAFETY: the CPU supports the features.
			return unsafe { self.advance_v3(handler, bytes) };
		}
		self.advance_with(handler, bytes);
	}

	/// Callable only where the CPU has the features [`crate::cpu::has_v3`]
	/// checks.
	#[cfg(target_arch = "x86_64")]
	#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,fma")]
	pub(crate) fn advance_v3<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
		self.advance_with(handler, bytes);
	}

	/// The body of [`Parser::advance`], compiled into each variant.
	#[inline(always)]
	pub(crate) fn advance_with<H: Handler>(&mut self, handler: &mut H, mut bytes: &[u8]) {
		if self.partial_len > 0 {
			bytes = self.finish_partial(handler, bytes);
		}
		while !bytes.is_empty() {
			let used = match self.state {
				State::Ground => self.ground(handler, bytes),
				State::Osc => self.osc_string(handler, bytes),
				State::DcsPassthrough => self.dcs_passthrough(handler, bytes),
				State::Apc | State::IgnoredString | State::DcsIgnore => self.ignored_string(handler, bytes),
				_ => self.sequence(handler, bytes),
			};
			bytes = &bytes[used..];
		}
	}

	/// Ground state: text runs and C0 controls, until ESC or the end.
	#[inline(always)]
	fn ground<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
		let mut used = 0;
		while used < bytes.len() {
			let rest = &bytes[used..];
			// Sequences often follow each other; only scan when text starts here.
			if rest[0] >= 0x20 {
				let (len, kind) = simd::text_run(rest);
				if kind == simd::Text::Ascii {
					handler.print_ascii(&rest[..len]);
				} else {
					let c1 = kind == simd::Text::MaybeC1;
					self.text(handler, &rest[..len], len == rest.len(), c1);
				}
				used += len;
				continue;
			}
			used += 1;
			match rest[0] {
				0x1b if rest.get(1) == Some(&b'[') => {
					// CSI, the most common sequence: parse it without leaving.
					self.enter_escape();
					self.enter_csi();
					used += 1;
					used += self.csi_params(handler, &bytes[used..]);
					if self.state != State::Ground {
						return used;
					}
				}
				0x1b => {
					self.enter_escape();
					return used;
				}
				byte => handler.execute(byte),
			}
		}
		used
	}

	/// Text containing DEL or non-ASCII bytes. Malformed UTF-8 prints U+FFFD
	/// per maximal invalid sequence, except that a lone byte `0x80..=0x9f`
	/// executes as a C1 control. An incomplete character at the end of the
	/// read (`at_end`) is kept for the next one; before a control it is
	/// malformed.
	#[inline(always)]
	fn text<H: Handler>(&mut self, handler: &mut H, run: &[u8], at_end: bool, c1: bool) {
		if let Some(text) = Text::new(run) {
			return print_valid(handler, text, c1);
		}
		// Each chunk is valid text, then at most one malformed sequence: the
		// longest prefix of a character, or a byte no character starts with.
		let mut chunks = run.utf8_chunks().peekable();
		while let Some(chunk) = chunks.next() {
			print_valid(handler, chunk.valid().into(), c1);
			let invalid = chunk.invalid();
			let incomplete = || std::str::from_utf8(invalid).is_err_and(|e| e.error_len().is_none());
			if invalid.is_empty() {
				continue;
			} else if chunks.peek().is_none() && incomplete() {
				// A character cut off at the end of the run.
				if at_end {
					self.partial[..invalid.len()].copy_from_slice(invalid);
					self.partial_len = invalid.len();
				} else {
					handler.print(Text::REPLACEMENT);
				}
			} else if let [byte @ 0x80..=0x9f] = invalid {
				handler.execute(*byte);
			} else {
				handler.print(Text::REPLACEMENT);
			}
		}
	}

	/// Completes the character held from the previous read with the start
	/// of this one; returns the bytes after it.
	fn finish_partial<'a, H: Handler>(&mut self, handler: &mut H, bytes: &'a [u8]) -> &'a [u8] {
		let held = self.partial_len;
		let mut joined = [0u8; 8];
		let take = bytes.len().min(4 - held);
		joined[..held].copy_from_slice(&self.partial[..held]);
		joined[held..held + take].copy_from_slice(&bytes[..take]);
		let joined = &joined[..held + take];
		self.partial_len = 0;
		let len = utf8_len(joined[0]);
		match std::str::from_utf8(joined) {
			Err(error) if error.valid_up_to() < len => match error.error_len() {
				// Still incomplete, and this read is used up.
				None => {
					self.partial[..joined.len()].copy_from_slice(joined);
					self.partial_len = joined.len();
					&[]
				}
				// The held bytes were a valid prefix, so the error is in this read.
				Some(error_len) => {
					handler.print(Text::REPLACEMENT);
					&bytes[error_len - held..]
				}
			},
			_ => {
				// `from_utf8` found at least this character valid.
				let text = Text::new(&joined[..len]).unwrap_or(Text::REPLACEMENT);
				print_valid(handler, text, true);
				&bytes[len - held..]
			}
		}
	}

	fn enter_escape(&mut self) {
		self.state = State::Escape;
		self.intermediate_len = 0;
		self.ignore = false;
	}
}

/// Byte length of the character that `lead` starts.
fn utf8_len(lead: u8) -> usize {
	match lead {
		0xf0.. => 4,
		0xe0.. => 3,
		0xc0.. => 2,
		_ => 1,
	}
}

/// Prints valid text, executing the C1 controls U+0080..=U+009F in it
/// unless `c1` says there are none.
#[inline(always)]
fn print_valid<H: Handler>(handler: &mut H, mut text: Text<'_>, c1: bool) {
	while let Some(at) = c1.then(|| c1_position(text.as_bytes())).flatten() {
		let (before, control) = text.split_at(at);
		if !before.is_empty() {
			handler.print(before);
		}
		handler.execute(control.as_bytes()[1]);
		text = control.split_at(2).1;
	}
	if !text.is_empty() {
		handler.print(text);
	}
}

/// Byte offset of the first encoded C1 control (`c2 80..=9f`).
#[inline]
fn c1_position(bytes: &[u8]) -> Option<usize> {
	let mut from = 0;
	while let Some(at) = bytes[from..].iter().position(|&byte| byte == 0xc2) {
		let at = from + at;
		if (0x80..=0x9f).contains(&bytes[at + 1]) {
			return Some(at);
		}
		from = at + 1;
	}
	None
}
