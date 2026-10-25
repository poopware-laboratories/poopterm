# Replacing vte: design for an in-house escape-sequence parser

Status: **implemented** in `src/vt` (phases 0–3: tokenizer, differential
tests, switch-over; vte is now a dev-dependency only). APC delivery for the
kitty graphics protocol is in place, and so are the later fast paths this
plan proposed: SIMD UTF-8 validation (`vt/utf8.rs`), the generated width
table (`src/width.rs`) and narrow multibyte runs printed sixteen bytes at a
time (`parser/narrow.rs`). The text below is the original plan, kept for
its reasoning; `docs/architecture.md` describes what was built.
This document describes how Poopterm will replace the `vte` crate (currently `vte = "^0.15"`) with
its own batch-oriented, SIMD-accelerated parser, and how to get there in
small steps that each ship on `main`.

## 1. Motivation and goals

### Where the parser is today

`src/parser/mod.rs` wraps `vte::Parser` and routes its `Perform` callbacks
to the `csi`, `esc`, `osc`, `sgr` and `print` submodules. Around vte sit
several workarounds:

| Workaround | Why it exists | Where |
|------------|---------------|-------|
| Ground-state fast path (`Batch::ground_fast_path`) | vte calls `print` once per character | `parser/mod.rs`, `parser/scan.rs` |
| `ground` flag, `returned_to_ground`, `advance_until_terminated` | we cannot ask vte which state it is in | `parser/mod.rs` |
| `fast_csi::apply` for `m H K A-D` | vte's CSI path is slow for the most common sequences | `parser/fast_csi.rs` |
| UTF-8 carry (`carry`, `complete_prefix`) | vte 0.15 drops bytes after a character split across `advance` calls | `parser/mod.rs` |
| Own UTF-8 decoder; invalid input falls back to vte | to keep replacement semantics identical | `parser/utf8.rs` |
| 1 KiB `Batch::run` buffer | turns per-char `print` back into runs for `Term::print_ascii` | `parser/mod.rs` |

The fast path is already where most bytes go. The remaining cost is what
vte still handles, and the bookkeeping of switching in and out of it:

- On Unicode output, vte's `ground_dispatch` plus UTF-8 validation take
  ~55% of parser time (`docs/performance.md`).
- Parser-only figures (200x50 grid): ASCII 131 MB/s, Unicode 57 MB/s,
  CSI-heavy 110 MB/s. End to end: ASCII ~100, Unicode ~65, CSI ~112,
  cursor movement ~58 MB/s.
- The PTY producer path alone sustains 300+ MB/s (`scripts/bench-pty.py`),
  so the parser and term mutation are the ceiling.
- DCS payloads (Sixel) arrive one byte at a time through `put`, which then
  pushes into `sixel::Transfer`. There is no APC handling at all, so the
  kitty graphics protocol cannot be added without parser support.

### Goals

1. **At least 200 MB/s end to end** (higher wherever it stays safe) on every payload in `bench-throughput.py`
   (ASCII, Unicode, SGR-heavy, cursor movement) on the reference hosts in
   `docs/performance.md`, with the parser alone well above that.
2. **Image protocols as fast as possible**, with no fixed target: Sixel (DCS)
   and the kitty graphics protocol (APC, base64) stream through the parser
   in slices, not bytes, with SIMD base64 decoding.
3. **Span-based API**: the parser hands `Term` runs of printable text and
   whole sequences, never single characters.
4. **Explicit, inspectable state**: the parser can report its state, so the
   `ground` guessing and `advance_until_terminated` loop disappear.
5. **Correct across read boundaries** without the UTF-8 carry hack.
6. **Hardware-aware**: use AVX2/AVX-512 on x86_64 and NEON on aarch64 where
   they help, with a portable fallback that is always correct.
7. **Same bounds on untrusted input** as today, enforced in one place.

### Non-goals

- Changing terminal semantics. Any behavior change (new sequences, fixed
  bugs) lands separately, after the parser swap, with its own tests.
- Supporting encodings other than UTF-8, or 8-bit C1 controls in UTF-8
  mode beyond what vte does (see section 2).
- A general-purpose crate for other projects. The API is shaped for `Term`.
- Implementing the kitty graphics protocol itself; this plan only makes the
  parser capable of carrying it efficiently.

## 2. Compatibility contract

During migration the new parser must produce **the same sequence of
`Perform` calls as vte 0.15, byte for byte**, for every input and every way
of splitting it into reads (with the documented exception of vte's
split-character bug, where we match vte fed whole characters, which is what
we do today). Once vte is gone the contract is frozen as the differential
corpus plus the emulation tests.

### States

The state machine is Paul Williams' DEC ANSI parser
(vt100.net/emu/dec_ansi_parser), as vte implements it:

| State | Entered by | Leaves on |
|-------|-----------|-----------|
| `ground` | default, after any dispatch | ESC, C0 executes in place, 0x80+ starts UTF-8 |
| `escape` | ESC from anywhere (except string states, see below) | intermediate 0x20-0x2f, final 0x30-0x7e, `[` `]` `P` `X` `^` `_` |
| `escape_intermediate` | 0x20-0x2f in `escape` | final 0x30-0x7e -> `esc_dispatch` |
| `csi_entry` | `ESC [` | params, private markers `< = > ?`, intermediates, final |
| `csi_param` | 0x30-0x39, `;`, `:` | intermediate, final, or invalid -> `csi_ignore` |
| `csi_intermediate` | 0x20-0x2f | final; 0x30-0x3f -> `csi_ignore` |
| `csi_ignore` | malformed CSI | final 0x40-0x7e (no dispatch) |
| `dcs_entry`, `dcs_param`, `dcs_intermediate`, `dcs_ignore` | `ESC P` | final -> `hook`, then `dcs_passthrough` |
| `dcs_passthrough` | after `hook` | `put` per byte; ST/CAN/SUB/ESC -> `unhook` |
| `osc_string` | `ESC ]` | BEL or ST -> `osc_dispatch` |
| `sos_pm_apc_string` | `ESC X`, `ESC ^`, `ESC _` | ST/CAN/SUB; vte ignores contents |

Our parser adds one observable difference only after phase 3: APC contents
are delivered (for kitty graphics) instead of dropped. Until then APC is
ignored exactly as vte does.

### Rules to match exactly

- **C0 controls** execute in every state except the string states, without
  leaving the current state (e.g. `\n` inside a CSI). DEL (0x7f) is ignored
  in sequences; in ground vte treats it as described by its tables, and the
  fast path already routes it to the general path.
- **C1 controls**: vte decodes input as UTF-8, so raw 0x80-0x9f bytes are
  not C1 controls; they are invalid UTF-8 and print U+FFFD. Encoded C1
  (U+0080-U+009F as two bytes) is treated as vte does (print vs execute must
  be pinned by a differential test, not assumed).
- **CAN (0x18) and SUB (0x1a)** abort any sequence and return to ground;
  they are also executed (vte calls `execute`). `Batch::execute` relies on
  this.
- **ESC interrupts** any sequence and starts a new one. In `osc_string` and
  DCS passthrough, ESC is the first half of ST; the following `\` completes
  it, any other byte is processed as a new escape. An ESC-terminated OSC
  leaves the parser in `escape` until the `\` arrives (today's
  `returned_to_ground` logic depends on this).
- **Parameters**: at most 32 params (vte's `MAX_PARAMS`); values saturate
  at `u16::MAX`; empty params are 0; `:` starts subparameters of the
  current param (`Params::iter` yields slices). Overflowing the param count
  sets `ignore` but still dispatches, as vte does.
- **Intermediates**: at most 2; more sets `ignore`.
- **OSC**: params split on `;`, at most 16 params (vte's limit; the rest are
  joined into the last); raw buffer bounded (vte grows a Vec; we cap it, see
  section 3). Terminated by BEL (`bell_terminated = true`) or ST. Bytes
  0x80+ are accepted as-is (UTF-8 titles); `osc.rs` validates UTF-8 itself.
- **Invalid UTF-8** in ground prints U+FFFD per maximal invalid subpart,
  following vte's (and `std`'s `Utf8Chunks`) replacement behavior. A
  sequence interrupted by ESC or a C0 control emits U+FFFD and then the
  control. These cases need explicit differential tests.

### Bounds on untrusted input (unchanged, centralized)

| Input | Limit today | Owner after migration |
|-------|-------------|-----------------------|
| CSI params / intermediates | 32 / 2 (vte) | parser constants |
| OSC buffer | unbounded Vec in vte | parser: 1 MiB cap, then ignore until terminator |
| OSC 52 clipboard | 1 MiB decoded (`osc.rs`) | stays in `osc.rs`; reads stay ignored |
| Sixel payload / size | 4 MiB, 4096 px side, 16 Mpx (`sixel.rs`) | stays in `sixel.rs` |
| Combining marks, hyperlinks | `MAX_MARKS = 8`, u16 ids (`term::extras`) | unchanged |
| APC (kitty graphics) | not parsed | new per-image and total caps, defined in phase 3 |

The parser never allocates proportionally to input except in bounded string
buffers, and never buffers DCS/APC payloads itself.

## 3. Architecture

### API: spans and sequences, not characters

```rust
pub(crate) trait Sink {
	fn print_ascii(&mut self, run: &[u8]);          // 0x20..=0x7e only
	fn print_utf8(&mut self, run: &str);            // valid, no controls
	fn print_char(&mut self, c: char);              // U+FFFD and edge cases
	fn execute(&mut self, byte: u8);
	fn csi(&mut self, seq: &Csi<'_>);               // params, intermediates, ignore, final
	fn esc(&mut self, intermediates: &[u8], ignore: bool, byte: u8);
	fn osc(&mut self, params: &[&[u8]], bell_terminated: bool);
	fn dcs_start(&mut self, seq: &Csi<'_>);
	fn dcs_data(&mut self, chunk: &[u8]);           // slices, possibly many per read
	fn dcs_end(&mut self);
	fn apc_start(&mut self) {}                      // phase 3
	fn apc_data(&mut self, _chunk: &[u8]) {}
	fn apc_end(&mut self) {}
}

impl Parser {
	pub(crate) fn advance(&mut self, sink: &mut impl Sink, bytes: &[u8]);
	pub(crate) fn state(&self) -> State;
}
```

`Term` implements `Sink` directly; `Term::print_ascii` and
`Term::print_utf8` already take runs. During migration an adapter
`VteCompat<P: vte::Perform>` turns the span calls back into vte-style
calls so the differential harness can compare call logs.

### Core loop

```
loop {
	match state {
		Ground  => scan with SIMD to next "interesting" byte; emit run; handle it
		String  => scan to next terminator candidate (ESC, BEL, CAN, SUB, C0); emit slice
		_       => table-driven step, one byte at a time (sequences are short)
	}
}
```

Long runs (text, string payloads) go through SIMD scans; short control
sequences go through a compact transition table.

### Transition table

- `TABLE: [[Transition; 256]; STATE_COUNT]`, one `u8` per entry encoding
  `(action, next_state)` (4 bits each), generated by a `const fn` from the
  Williams state chart so it is reviewable as code, not as numbers.
  ~14 states x 256 = 3.5 KiB, fits in L1.
- "Anywhere" transitions (CAN, SUB, ESC, C1 when applicable) are folded
  into every row at build time.
- Actions: `print`, `execute`, `collect`, `param`, `esc_dispatch`,
  `csi_dispatch`, `hook`, `put`, `unhook`, `osc_start/put/end`, `clear`,
  `ignore`. The loop body is a `match` on action, kept small enough to
  inline.
- A CSI fast lane keeps what `fast_csi.rs` does today (digits and `;`
  straight to a `[u16; 32]`, then dispatch on common finals) but as part of
  the parser, so no rollback-to-ESC path is needed: state is kept when the
  sequence spans a read.

### SIMD classification

The ground scan finds the first byte that is `< 0x20`, `== 0x7f` or
`>= 0x80` (ESC is in the first class). `scan::printable_prefix` already
does this with `u8x32` and `std::simd` (nightly `portable_simd`, pinned by
`rust-toolchain.toml`). Extensions:

| Scan | Stops at | Used in |
|------|----------|---------|
| `printable_prefix` | `< 0x20`, `0x7f`, `>= 0x80` | ground, ASCII |
| `utf8_text_prefix` | `< 0x20`, `0x7f`; validates UTF-8 on the way | ground, Unicode |
| `string_prefix` | ESC, BEL, CAN, SUB (and C0 for OSC) | OSC, DCS, APC |
| `base64_decode` | non-alphabet byte | kitty graphics, OSC 52 |

Width choice and dispatch:

- Compile-time: `u8x32` by default (maps to AVX2 on x86_64-v3, two NEON
  ops on aarch64); `u8x64` behind `target_feature = "avx512bw"`.
- Runtime: one `std::is_x86_feature_detected!` check at `Parser::new`
  selects a function pointer per scan (`avx512`, `avx2`, `sse2`); aarch64
  always has NEON. Each variant is `#[target_feature(enable = ...)]` over
  the same generic `std::simd` code, so there is one implementation.
- Only keep a wider variant if the benchmark shows a gain; AVX-512
  frequency effects on some x86 parts can make it slower for short runs.
- Short inputs (< 32 bytes, typical for interactive use) use a scalar
  loop; the vector setup must not cost latency on keystroke echo.

### ASCII and UTF-8 fast paths

- **ASCII**: unchanged in spirit: `print_ascii(run)` on whatever
  `printable_prefix` returns. Execute C0, continue.
- **UTF-8**: done. Whole runs are validated with a SIMD validator (the
  lookup-table approach of Keiser–Lemire, about 3 shuffles per 16 bytes,
  AVX2 chosen at runtime), stopping at controls; unicode gained about 30%.
  Valid runs go to `print(&str)`; an invalid run falls back to the scalar
  path, which emits U+FFFD exactly as vte does.
- **Width lookup**: done. `src/width.rs` reads a two-level table
  (`[u8; 0x110000 >> 7]` block numbers to 128-entry blocks of 2-bit widths,
  13.5 KiB) that `scripts/width_table.py` generates from the Unicode data
  files, so Term no longer calls `UnicodeWidthChar` per scalar. CJK and
  emoji runs gained about 15%.

### Chunk boundaries and resumable state

Every state is resumable at every byte; there is no lookahead:

- CSI/ESC: params, intermediates and partial number live in the parser.
- UTF-8: up to 3 pending bytes plus expected length live in the parser,
  replacing `carry` / `complete_prefix` in `parser/mod.rs`.
- OSC: bytes accumulate in a bounded `Vec<u8>` reused across sequences.
- DCS/APC: no buffering; each read's slice goes straight to the sink.
- A pending ESC inside a string state (possible first half of ST) is one
  flag.

Test requirement: for every corpus input, every split point (and random
multi-splits) yields the same call log as feeding it whole (section 5).

### Streaming DCS and APC payloads

- `dcs_data(&[u8])` replaces per-byte `put`. `sixel::Transfer::push`
  becomes `extend(&[u8])`, keeping its `MAX_PAYLOAD` check per slice.
- APC (`ESC _ G ... ST`) for kitty graphics: the parser reports the
  control-data prefix (up to `;`) once, then payload slices. A streaming
  base64 decoder (SIMD, 4-to-3 bytes with `swizzle_dyn` lookups) carries at
  most 3 leftover chars between slices. Decoded bytes go to an image
  assembler with explicit caps, never to an unbounded buffer.
- OSC 52 can reuse the same base64 decoder; its 1 MiB limit stays.

### Memory bounds

Parser state is fixed size apart from the OSC buffer (capped at 1 MiB,
then the sequence is ignored to its terminator, as an oversize CSI is) and
the param array. No allocation happens in ground state or for CSI/ESC.

## 4. Integration plan

Each phase is a series of small commits on `main`; each leaves CI green and
the terminal fully working. vte stays a dependency until phase 4.

| Phase | Scope | Exit criteria |
|-------|-------|---------------|
| 0 | Harness: call-log recorder for `vte::Perform`, corpus, split tester, `cargo-fuzz` target, image payloads in benchmarks | harness runs in `cargo test`; fuzz target builds; baseline numbers recorded |
| 1 | New parser for ground state and printing: SIMD UTF-8 validation, width table, `print_utf8(&str)`; vte still handles ESC onward | differential clean; Unicode parser-only throughput up measurably; `batched_printing_matches_per_character_printing` green |
| 2 | Table-driven ESC and CSI states incl. colon subparams; delete `fast_csi.rs`, `ground` flag, `advance_until_terminated` loop | differential clean on all splits; CSI and cursor payloads no slower |
| 3 | OSC, DCS, SOS/PM/APC; slice-based `dcs_data`; streaming base64; Sixel throughput benchmark | differential clean (APC still ignored); Sixel MB/s measured; then APC delivery behind its own tests |
| 4 | Remove vte: `Term` implements `Sink` only; drop `vte::Params` from `csi.rs`/`sgr.rs` signatures; keep recorded call logs as golden files | `vte` gone from `Cargo.toml`; docs updated (`architecture.md`, `performance.md`, AGENTS.md rule about vte agreement rewritten against the golden corpus) |

Notes per module:

- `parser/mod.rs` shrinks to the `Sink` impl for `Term` and the
  synchronized-output routing (`Term::pending`); the state machine moves
  to new files (`parser/machine.rs`, `parser/table.rs`, `parser/simd/*`),
  each under ~400 lines.
- `csi.rs`, `sgr.rs` keep their logic; only the params type changes
  (a `Csi` view with the same `iter()` over subparam slices as
  `vte::Params`, so the diff is mechanical).
- `scan.rs`, `utf8.rs` grow into `parser/simd/`.
- `Term::print_ascii` / `print_utf8` keep their contracts; any change to
  them still requires the batched-vs-per-character test.

## 5. Testing and verification

- **Differential testing against vte** (phases 0-3): feed the same bytes
  to vte with a recording `Perform` and to the new parser through
  `VteCompat`, compare call logs. Inputs:
  - a hand-written corpus of edge cases (every row of section 2, each state
    interrupted by ESC/CAN/SUB/C0, max params, colon subparams, OSC with BEL
    and ST, invalid and truncated UTF-8, encoded C1);
  - recorded real output (vim, htop, less, tmux, `ls --color`, Sixel and
    kitty-graphics samples);
  - property tests generating random streams biased toward ESC, `[`, `]`,
    `P`, `;`, `:`, digits and 0x80+ bytes.
  Each input is checked whole, split at every byte, and split randomly
  into many pieces.
- **Fuzzing**: a `cargo-fuzz` target (`fuzz/` crate, not built by CI by
  default) runs the differential check and asserts no panics, bounded
  memory, and identical logs. Crashes become corpus entries.
- **Existing tests**: emulation tests in `src/term/tests.rs` and parser
  unit tests stay green unchanged; `batched_printing_matches_per_character_printing`
  gains fragments for UTF-8 runs, split sequences and DCS slices.
- **Conformance**: run vttest (cursor, screen features, character sets)
  and esctest against release builds before and after each phase; record
  differences in this doc. These are manual checks, not CI gates.
- **Visual checks** for anything touching printing, per
  `docs/testing.md#visual-checks`.
- **Platforms**: SIMD variants are `cfg`-gated, so run clippy for
  `aarch64-apple-darwin` and `x86_64-unknown-freebsd` as AGENTS.md
  requires, and run the test suite with runtime dispatch forced to each
  variant (an env var or test hook) so the scalar path is always tested.

## 6. Benchmarking methodology

Follow `docs/performance.md`: measure before and after, report medians and
ranges, and drop changes without a reproducible gain.

- Parser only: `cargo test --release -- --ignored --nocapture parser_throughput`.
- End to end: `python3 scripts/bench-throughput.py` (DA1 round-trip ends
  each timer).
- Producer ceiling: `python3 scripts/bench-pty.py`, to confirm the parser,
  not the PTY, is the limit.
- Profiles with `perf` as described in `docs/performance.md`.

Results table to fill per phase (MB/s, median of 64 MiB x 3, same host and
window size):

| Payload | Parser before | Parser after | End to end before | End to end after |
|---------|---------------|--------------|-------------------|------------------|
| ASCII | 131 | | ~100 | |
| Unicode | 57 | | ~65 | |
| CSI-heavy | 110 | | ~112 | |
| Cursor movement | | | ~58 | |
| Sixel (to add) | | | | |
| Kitty graphics, base64 (to add) | | | | |
| OSC-heavy (titles, hyperlinks; to add) | | | | |

New payloads go into both `parser_throughput` (its `payload(kind)`
function) and `bench-throughput.py`. Image payloads measure decoded pixels
per second as well as input bytes, and must not trip the existing size
limits. Interactive latency (small reads) is checked too, so the SIMD setup
cost does not regress echo.

## 7. Risks and open questions

- **Exact vte quirks**: some vte behavior may be accidental (C1 handling,
  OSC param joining, DEL in sequences). We match it during migration and
  decide afterwards, one behavior change per commit.
- **Split-character bug**: we must decide whether the differential harness
  compares against vte fed whole characters (current behavior) or raw
  splits; the former is the plan.
- **AVX-512 value**: may not pay off for terminal-sized runs; keep only
  with measurements. Runtime dispatch adds a check or an indirect call per
  scan, which must be cheaper than the work it saves. Done since (#36): the
  tokenizer, printing, UTF-8 validation, cell fills and glyph blending
  choose an x86-64-v3 copy at runtime (see [`unsafe.md`](unsafe.md)).
- **Where the time really goes**: once parsing is faster, `Term` mutation
  (row writes, scrolling, line feeds on ASCII) may dominate; 200 MB/s may
  need Term work that is out of scope here.
- **Width table drift**: the generated table must follow the pinned
  `unicode-width` version; `width::tests` compares every scalar.
- **Kitty graphics scope**: transmission modes (direct, file, shared
  memory), chunking (`m=1`) and placement are a separate design; the
  parser only promises streaming APC.
- **Maintenance cost**: we take on a component vte maintained for us. The
  golden corpus and fuzz target are what keep it honest after vte is gone.
- **Open**: should OSC also stream (for very long OSC 52 or hyperlinks), or
  is a capped buffer enough? Should the parser run directly on the
  `OutputQueue` buffers to avoid a copy (see the TODO on PTY input copies)?
