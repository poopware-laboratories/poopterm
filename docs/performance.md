# Performance

## Measuring

All benchmarks share one deterministic corpus (`src/bench/corpus.rs`) of
what programs actually send a terminal; `poopterm --benchmark --list` shows
it: build logs, colored compiler output with hyperlinks, `ls --color`,
highlighted source, accented/Greek/Cyrillic prose, CJK, emoji with ZWJ
sequences and combining marks, full-screen TUI redraws in synchronized
output, editor scrolling with scroll regions, progress bars, truecolor
half-block art, cursor motion, shell prompts with OSC 133/7/0/8, Sixel, and
kitty graphics (raw RGBA, PNG, zlib). Results are per workload plus an
overall figure (total bytes over total time), so a regression in one use
case cannot hide behind another.

**Headless**, parser and terminal, then with a frame rendered every MiB.
Each run starts with a full scrollback, as after real use, so costs that grow
with history show up:

```sh
cargo build --release
./target/release/poopterm --benchmark                      # 16 MiB per workload, 120×40
./target/release/poopterm --benchmark --mib 64 --only log,tui --size 200x50
./target/release/poopterm --benchmark --kib 64 --read 16 --frame-every 64  # interactive
```

`--read` sets the bytes per parser call (default 64 KiB, one PTY read) and
`--frame-every` the bytes between frames (default 1 MiB). Small values model
interactive programs, which write a few bytes at a time and get a frame for
most writes.

**End to end**, in any terminal (also for comparing emulators):

```sh
python3 scripts/bench-throughput.py                        # 16 MiB × 3 runs per workload
python3 scripts/bench-throughput.py --mib 64 --repeat 5 --only log --only emoji
python3 scripts/bench-throughput.py --format json --output results.json
```

It takes the workloads from `poopterm --bench-corpus`, generated for the
window's size, so every terminal gets identical bytes. Each timer stops only
after a DA1 (`CSI c`) round-trip proves the terminal parsed everything. It
reports the median and min–max of the runs plus the window size; compare
terminals at the same window size, since grid size affects the cost.
The script uses `target/release/poopterm` when present, otherwise it finds
`poopterm` on `PATH`; set `POOPTERM_BIN` to choose a different build.

The terminal is named by its XTVERSION reply (`CSI > q`), not `TERM_PROGRAM`,
which a terminal launched from another one inherits. Every write, including
the warm-up, ends on a whole escape sequence and UTF-8 character: a DA1
query that lands inside an unfinished `CSI` is answered by Poopterm and
xterm, but dropped by kitty, which used to abort the run.

**Parser only**, without rendering: `cargo test --release -- --ignored
--nocapture parser_throughput`. **PTY ceiling**: `scripts/bench-pty.py`.
**Inflate**: `inflate_throughput`, the same way (our decoder against
`miniz_oxide`). The other benchmarks are listed in
[`testing.md`](testing.md#running).

Run-to-run noise on one machine is a few percent, more than many single
changes gain. Compare by alternating the two builds several times
(A B B A …) and report the medians with their spread; for changes under
about 2%, also compare instruction counts with `perf stat -e
instructions:u`, which barely vary between runs.

To profile, build the `profiling` profile (release plus symbols) and use
`perf`. User-space events work without root at the default
`kernel.perf_event_paranoid=2`; to see kernel time too (page faults, PTY
reads), lower it for the session with `sudo sysctl -w
kernel.perf_event_paranoid=1 kernel.kptr_restrict=0`.

```sh
cargo build --profile profiling
perf record -e cycles:u -- ./target/profiling/poopterm --benchmark --only emoji
perf report --sort sym
```

Where `perf` is unavailable, callgrind works on the same build (slower, but
exact instruction counts):

```sh
valgrind --tool=callgrind ./target/profiling/poopterm --benchmark --only kitty-png --mib 4 --no-render
callgrind_annotate --inclusive=yes callgrind.out.<pid> | head -40
```

The profile includes generating the corpus; look at the costs under
`bench::measure`.

### Profile-guided optimization

`scripts/pgo.sh` (or the `make pgo*` targets) builds an instrumented
binary, trains it, merges the profiles with the toolchain's own
`llvm-profdata` (from the `llvm-tools` component, added if missing) and
rebuilds with them. No other tools are needed.

```sh
make pgo             # instrument, train on the benchmark corpus, build optimized
make pgo-use         # dogfood the instrumented terminal; closing it adds a profile
make pgo-build       # rebuild from every profile collected so far
make bolt            # Linux: profile the PGO build and lay it out with BOLT
make pgo-compare     # release vs PGO vs BOLT, same benchmark
make pgo-install     # build what releases ship and install it system-wide
make pgo-clean       # forget the profiles
```

`poopterm --benchmark` needs no window and covers every workload, the
parser, terminal state and rendering, so it is the base training run
(`PGO_MIB`, default 8, sets its size). A second run feeds every workload in
16-byte reads with a frame every 64 bytes, so interactive paths (key echo,
prompts, frequent frames) are trained too, not only floods. Sessions with `make pgo-use` add the
paths only a window exercises: input, tabs, the UI and real programs.
Profiles accumulate under `target/pgo/profiles` until `make pgo-clean`;
after large code changes, clean and retrain.

Released Debian and RPM packages ship the PGO build: the release job runs
`scripts/pgo.sh dist`, which instruments, trains and rebuilds, then puts the
result at `target/release/poopterm`. If any step fails, it builds a plain
release binary instead and logs a warning. `make pgo-install` builds and
installs the same binary. On an i3-N305 (headless, medians
of three alternating runs) the trained build is 5% faster overall: cursor
+14%, editor +14%, shell +12%, source +10%, tui +10%, compiler +9%, and
emoji −2.5%. Fed interactively (`--read 16 --frame-every 64`) it is 13%
faster.

### BOLT

On Linux, `make bolt` (and `dist`, when `llvm-bolt` and `merge-fdata` are
installed: Fedora `llvm-bolt`, Debian and Ubuntu `bolt-<version>`, named
through `LLVM_BOLT` and `MERGE_FDATA` when they carry the version) lays the
PGO build out again. The PGO build links with `--emit-relocs` and keeps its
symbols so BOLT can move code; `bolt` instruments it, runs the same training,
merges the profiles and rewrites it: basic blocks by ext-TSP, functions by
cache-directed sort, cold code split off, identical functions folded. The new
code goes into the old text section without huge-page alignment, so the
binary keeps the PGO build's size (5.2 MB); `target/pgo/optimized` keeps the
unbolted build for comparison, and `make pgo-compare` runs all three. If BOLT
fails, `dist` ships the PGO build with a warning; without the tools it says
so and does the same.

Instrumentation works everywhere, CI VMs included, which have no LBR. On the
i3-N305 (medians of three alternating runs, PGO against PGO + BOLT) BOLT adds
2.9% overall and 2.3% fed interactively: shell +9.6%, compiler +7.0%, sixel
+6.5%, truecolor +5.0%, cjk +3.8%, cursor +3.1%; kitty-png −0.9% is within
noise.

## Results

Intel i3-N305 (8 E-cores, AVX2), Linux, plain release build (no PGO).
**Headless** is `poopterm --benchmark --mib 16 --no-render` at 120×40,
starting from a full scrollback, median of 3 runs: first at 26.10.3 plus
the synchronized-output and combining-mark changes below (`d48ebd6`), then
at 26.10.42 with our own inflate (#57), and at 26.10.93 (after the
regression recovery, runtime CPU dispatch and the emoji work). **End to end** is
`scripts/bench-throughput.py --mib 16 --repeat 3` in a Poopterm window of
100×30 cells (median MB/s), measured at `d48ebd6` and not yet repeated.

| Workload | End to end (`d48ebd6`) | Headless (`d48ebd6`) | Headless (26.10.42) | Headless (26.10.93) |
|----------|-----------:|---------:|---------:|---------:|
| log (plain ASCII) | 86 | 204 | 248 | 276 |
| compiler (colors, hyperlinks) | 77 | 122 | 153 | 188 |
| ls --color | 86 | 143 | 182 | 223 |
| source (highlighted) | 105 | 131 | 139 | 175 |
| unicode (accented, Greek, Cyrillic) | 83 | 92 | 138 | 154 |
| cjk | 86 | 102 | 154 | 173 |
| emoji (ZWJ, combining marks) | 63 | 69 | 120 | 135 |
| tui (sync blocks, htop-like) | 161 | 175 | 204 | 241 |
| editor (scroll regions) | 159 | 165 | 213 | 265 |
| progress bars | 175 | 190 | 286 | 371 |
| truecolor half blocks | 248 | 261 | 240 | 260 |
| cursor motion | 119 | 123 | 144 | 186 |
| shell prompts (OSC 133/7/0/8) | 90 | 115 | 142 | 153 |
| sixel | 72 | 76 | 149 | 152 |
| kitty-rgba | 301 | 1021 | 1023 | 1070 |
| kitty-png | 108 | 111 | 113 | 116 |
| kitty-zlib | 224 | 233 | 246 | 231 |
| **overall** | **108** | **133** | **175** | **198** |

End-to-end figures sit below headless ones where the PTY and the window
cost more than parsing: at `d48ebd6` plain ASCII parsed at 204 MB/s but
arrived at 86 MB/s, and raw kitty RGBA is limited by the PTY, not the
decoder. Where parsing dominates (emoji, Unicode, tui) the two are close.
Truecolor's headless figure has varied between 240 and 260 MB/s across
builds without a code change behind it. At 26.10.93 the workloads still
below 200 MB/s headless are emoji, sixel, shell, unicode, cjk, source,
kitty-png, cursor and compiler; between runs the medians move by a few
percent, so compare builds by alternating them (see above).

### Before 26.10

The first round of work, 0.43.0 to 26.10.0, on an X11 window at 100×30:

| Payload | 0.43.0 | 26.10.0 | Main change |
|---------|-------:|--------:|-------------|
| ASCII | 0.3 MB/s | ~100 MB/s | pipeline, pacing, row grid, SIMD fast path |
| Unicode | 0.3 MB/s | ~65 MB/s | pipeline, pacing, row grid, 16-byte cells |
| CSI-heavy | 0.3 MB/s | ~112 MB/s | pipeline, pacing |
| Cursor movement | 0.3 MB/s | ~58 MB/s | pipeline, pacing |

## What changed, and why

| Problem | Fix | Where |
|---------|-----|-------|
| The reader blocked on the UI thread for every 16 KiB read, and nearly every read triggered a full-window render. | Shared output queue with one wake-up per batch and backpressure at 1 MiB; redraws paced to the display. | `pty/queue.rs`, `app/frame_pacer.rs` |
| Each line feed memmoved the whole screen. | Row-based `Grid`; scrolling rotates rows and moves scrolled-off rows into the scrollback. | `term/grid.rs` |
| Cells were 52 bytes, because an inline `[char; 8]` was reserved for combining marks. | 16-byte cells; marks and hyperlinks interned in a side table. | `term/extras.rs` |
| Every printed character re-derived its style and checked clusters and wide characters. | ASCII runs printed with one cell template per row segment. | `parser/print.rs` |
| vte dispatches one callback per character. | SIMD ground-state fast path; batching on the vte path. Later replaced by the in-house tokenizer. | `src/vt` |
| Every erase repaired wide characters on every row. | Repair only the touched row. | `parser/csi.rs` |
| Each drawn cell did two SipHash lookups plus a fontdue lookup. | One FxHash lookup per cell. | `render/glyphs.rs` |
| `config.toml` was read from disk on every key press. | Shortcuts parsed once. | `input/shortcuts.rs` |

Measured and rejected: batching scrollback evictions through a row pool,
because it showed no reproducible end-to-end gain and slowed the parser
benchmark.

## Threads, damage tracking and floods (26.10)

Parsing moved to a thread per tab and rendering became damage-tracked (see
[`architecture.md`](architecture.md)). Findings worth knowing before changing
either:

- **Throughput was not the win; responsiveness was.** Before, the UI thread
  parsed, and during a flood it barely rendered: on the Unicode payload it
  drew 2 frames in the last second, with a 1.06 s freeze. Now it draws 29
  (max gap 39 ms) at the same throughput.
- **Full frames compete with parsing even on other cores.** A HiDPI window
  (2232×1432) is 12.8 MB per frame; drawing it at 60 fps during a flood cut
  Unicode throughput from 66 to 55 MB/s. The parser executed the same
  instructions but at lower IPC; pinning threads to separate L2 clusters made
  no difference, which points at the shared L3 and memory bandwidth. Hence
  floods render at 30 fps (`app::FramePacer::set_flooding`): measured at
  16/33/100 ms frame intervals, Unicode went 55/64/67 MB/s.
- **Fills into softbuffer's buffer were slow.** ~75% of frame time was
  plain fills straight into the X11 shared-memory segment. Rendering into a
  persistent back buffer and copying only changed rows avoids that.

32 MiB ×3 medians, 100×30, same session: previous build vs 26.10.

| Payload | Before | 26.10 |
|---------|-------:|------:|
| ASCII | 97.5 | 94.8 |
| Unicode | 66.7 | 64.8 |
| CSI-heavy | 108.7 | 106.6 |
| Cursor movement | 56.6 | 55.4 |

These differences are within run-to-run noise (the baseline's ASCII figure
ranged 89–97 MB/s over the day).

## Synchronized output and combining marks (26.10)

- **History-dependent costs were invisible.** Each headless run used a
  fresh terminal, so tui read 188 MB/s while a real terminal with a full
  scrollback managed 30. Every `?2026` block cloned the whole `Term`,
  scrollback and image stores included. Runs now start from a full
  scrollback, and a block freezes one `Frame` of the visible screen instead
  (`term/sync.rs`): tui went from 32 to about 180 MB/s headless.
- **Interning combining marks** hashed a freshly built entry with SipHash
  for every mark. Hashing in one write and looking up once (`term/extras.rs`)
  recovered a 10% regression; a 256-slot cache of `(id, mark) → id` in front
  of the table took emoji from 43 to 69 MB/s headless. SipHash stays,
  because programs choose the hashed content.
- **Titles** set by prompts on every line were filtered character by
  character; short printable-ASCII titles are now copied whole (shell +7%).
- **Profile-guided optimization** (`make pgo`, and every release) adds
  about 5% overall on top of all this, 13% when fed interactively; BOLT
  (`make bolt`) about 3% more.

## Follow-up work (26.10)

The TODO throughput work was implemented and validated with the parser and
terminal equivalence tests: UTF-8 decoding and width handling for common
ranges, scrollback row reuse, the empty-image fast path, and bounded parsing
for common CSI sequences. Poopterm preserves incomplete UTF-8 input across
feeds, including when a multibyte character is split between reads.

The PTY producer ceiling was measured on an Apple M4 with the discard-reader
benchmark in `scripts/bench-pty.py` (64 MiB ×3): ASCII 320, Unicode 323,
CSI-heavy 325, and cursor movement 305 MB/s. This confirms that the producer
path can exceed the 200 MB/s end-to-end goal on this host.

An owned-buffer queue prototype varied by payload in a 64 MiB ×5 comparison
(ASCII 213→266 MB/s, Unicode 242→225, CSI 189→194, cursor 162→165). A second
paired run did not complete, so the optimization was removed instead of
claiming an unproven gain. Selection and search row damage is checked by
pixel-equivalence tests; an ignored 300-frame test measured 25.6 ms for full
redraw and 3.39 ms for partial redraw at 100×30 cells and a 1000×600 surface.

## Parser, terminal and image work (26.10.28 onward)

Each change was measured headless at 16 MiB in alternating runs against
the `main` it was based on; gains are for the workloads it targeted.

| Change | Gain | Where |
|--------|------|-------|
| Scrollback as trimmed rows in one ring buffer, instead of a `VecDeque` of rows (#44) | compiler +22%, ls +13%, log +11% | `term/scrollback.rs` |
| SIMD UTF-8 validation (Keiser–Lemire, AVX2 at runtime) instead of `str::from_utf8` (#45) | unicode +31%, emoji +19%, cjk +19% | `vt/utf8.rs` |
| Blank only the written prefix of rows scrolled into history (#47) | compiler, ls, tui +5% | `term/grid.rs` |
| Fill cells with 32-byte AVX2 stores (#48) | editor +12%, progress +7% | `term/fill.rs` |
| Generated width table instead of `unicode-width`'s range chain (#49) | emoji +16%, cjk +15%, progress +15% | `src/width.rs` |
| Repeated marks answered without making the extras table unique (#50) | emoji +5% | `term/extras.rs` |
| Line feeds rotate row headers in one move (#51) | log, compiler, cjk +1–2% | `term/grid.rs` |
| Narrow multibyte runs printed sixteen bytes at a time (#52) | unicode +10% | `parser/narrow.rs` |
| Narrow and wide cell words built once per segment (#53) | cjk −3.5% instructions | `parser/print.rs` |
| History rows written with non-temporal stores, one `sfence` per unlock (#54) | log +10%, ls +4% | `term/scrollback.rs` |
| Marks joined to clusters without ending the print segment (#55) | emoji +12%, tui +6% | `parser/cluster.rs` |
| Sixel decoded into one flat buffer with alpha resolved up front (#56) | sixel +90% | `src/sixel.rs` |
| Our own inflate for kitty `o=z` (#57) | kitty-zlib +5% | `src/inflate` |
| Each grid row records its used extent: history pushes scan only that far, erases to the end stop there (#73) | emoji +13%, cjk +10%, log +10%, ls +10%, compiler +9%, unicode +9%, shell +7%, cursor +5%; truecolor −5% | `term/grid.rs` |

Measured and rejected along the way:

- A ring of rows for the screen grid, so a full-screen scroll only moves an
  index: scrolling workloads gained 3–6%, but cursor-addressed ones lost
  2–3%.
- Non-temporal stores with a fence per row: −11% (one fence per unlock is
  what landed in #54).
- Inlining the mark-joining helper: LLVM then spilled the character
  iterator in the CJK loop, costing 7% more instructions.

## Images (26.10)

Headless `--benchmark` on the i3-N305, 16 MiB per workload, 120×40, parser
and terminal (with a frame rendered every MiB in parentheses), in MB/s of
encoded input:

| Workload | Before the kitty protocol | Now |
|----------|-------------------------:|----:|
| kitty-rgba | ignored | ~1020–1110 (~700) | 1070 |
| kitty-zlib | ignored | ~246 (~210) | 231 |
| kitty-png | ignored | ~113 (~107) | 116 |

Before, APC strings were dropped unread, so those workloads only measured
the scan for their end. Decoding base64 with SIMD took kitty-rgba from about
954 to 1113 MB/s. Reserving the exact size of raw payloads up front showed no
reproducible gain and was dropped.

Zlib payloads now go through our own inflate (`src/inflate`), which beats
`miniz_oxide` on every stream in `inflate_throughput` except stored
(incompressible) data:

| Stream (zlib level 6) | Ours, MB/s | miniz_oxide, MB/s |
|-----------------------|-----------:|------------------:|
| RGB gradient 32×32 | 213 | 210 |
| RGB gradient 256×256 | 284 | 261 |
| random, 1 MiB (stored blocks) | 4962 | 6648 |
| words, 1 MiB | 1090 | 854 |
| runs, 1 MiB | 3544 | 2991 |
| short periods, 1 MiB | 1279 | 878 |

Small images spend most of their time building each stream's Huffman
tables, which is why kitty-zlib gains less than the large streams.

PNG time is still mostly the PNG crate: about 60% inflating (`fdeflate`)
and 20% unfiltering. Our own PNG decoder, on top of our inflate with a SIMD
unfilter, is the next step (see [`TODO.md`](../TODO.md)).

Sixel was the slowest workload at about 80 MB/s: each row was its own
`Vec`, resized on every paint, then copied into a bitmap with a second pass
to add alpha. Decoding into one flat buffer of final pixels doubled it
(#56).

## Where the time goes now

During a flood the parser thread is the bottleneck: it is ~100% busy and the
queue stays near full. Headless profiles at 26.10.42 (`perf record -e
cycles:u`, 16 MiB, shares of the whole run, which includes generating the
corpus) of the workloads still furthest below 200 MB/s:

| Workload | Largest costs |
|----------|---------------|
| emoji | `print_text` 30%, line feeds (`scroll_region_up`) 18%, `memmove` 11%, `join_marks` 10%, UTF-8 validation 5% |
| unicode | `print_text` 37%, line feeds 15%, `memmove` 11%, fills 6%, UTF-8 validation 4% |
| cursor | `Parser::advance` 25%, fills 11%, CSI final bytes 10%, `clear_range` 7%, CSI dispatch 7% |
| shell | `Parser::advance` 30%, line feeds 14%, `memmove` 9%, OSC dispatch 5%, `from_utf8` on OSC strings 5% |
| source | `Parser::advance` 36%, `memmove` 12%, SGR 12%, line feeds 10% |

Text workloads now spend most of their time writing cells and scrolling
(the `memmove` is mostly rows copied into history); escape-sequence
workloads spend it in `Parser::advance`, the per-sequence path that the
compatibility work of 26.10.23–26.10.27 made slower (see the regression
in [`TODO.md`](../TODO.md)). The plan toward 200 MB/s on every payload is
there too.

## Whole-cell ASCII stores (Apple M4)

The ASCII path assigned `Cell` fields separately, while the Unicode path
already built whole vector words. The ASCII writer now uses that same cell
representation, replacing only the character lane with a byte-derived scalar.
A safe wrapper checks equal lengths before the mutable word conversion;
layout, bounds, style and failure-path tests accompany the change.

On Apple M4, native release, identical saved Ghostty-generated ASCII,
120×80, four alternating processes per build, the isolated change measured
528→695 MB/s with 5000 history lines and 663→934 with history disabled.
After rebasing onto the SIMD UTF-8 fix, the same comparison measured
510→659 and 663→913 respectively. File loading, terminal creation and
teardown are included; generation is excluded. These are headless gains.

Six real software windows at 100×30, 5000 history lines and five passes per
file measured overlapping ASCII window medians: 246–253 before, 245–248
after. No windowed ASCII gain is established; the live output path remains
a separate bottleneck. Text and hyperlink snapshots matched on the saved
ASCII, raw/common-width UTF-8, styled, grapheme and numbered-history fixtures.

## Aarch64 UTF-8 validation (Apple M4)

A sample of Ghostty-generated UTF-8 with matching positive character widths
spent about a third of CPU samples in `str::from_utf8`: only x86 had a SIMD
validator. The aarch64 path now uses the existing lookup tables in safe
16-byte portable SIMD, lowering to NEON table lookups. Strings below eight
bytes keep the scalar path; a crossover microbenchmark found its setup
cheaper there. No parser, width, history, hyperlink or grapheme semantics change.

On an Apple M4, release with `-C target-cpu=native`, saved Ghostty-generated
files, 120×80, four alternating process measurements per build, fresh terminals
with 5000 history lines: common-width UTF-8 144→199 MB/s and mixed graphemes
164→239 MB/s. ASCII remained about 523→524 MB/s. The process timer includes
file loading, setup and teardown; both builds read identical files in 64 KiB
parser calls. The unfiltered random UTF-8 file has different display widths
between engines, so it is not used for an equal-state engine comparison.

In six alternating software-rendered windows at 100×30, 5000 history lines,
five timed passes per file per window, the medians of the three window
medians were 145→201 MB/s for common-width UTF-8 and 163→225 for graphemes.
Window noise was substantial: UTF-8 window medians ranged 115–145 before and
132–201 after. ASCII's ranges overlapped, so no windowed ASCII gain is claimed.
The timer waits for a DA1 reply after every payload.

The broader built-in corpus at 120×80, 16 MiB, no rendering, three alternating
runs per build improved from 387 to 416 MB/s overall. A separate 64 MiB check
of ASCII, truecolor and shell found truecolor within 1% and no repeatable
shell slowdown. These are host-specific observations, not a fastest-terminal
claim. Text and hyperlink snapshots matched before and after on the saved
ASCII, raw UTF-8, common-width UTF-8, styled, grapheme and retention fixtures.

## Local macOS CPU builds (26.10.132)

`make`, `make install` and local PGO builds use `-Ctarget-cpu=native` on
macOS. Rustc detects the processor and tunes dependencies as well as the
terminal. CI, cross compilation, staged packages and `pgo.sh dist` default
to portable architecture builds; `POOPTERM_PORTABLE=1` opts out locally.
Explicit `target-cpu` flags take precedence. This is a source-build policy,
not compilation at app startup.

Apple M4 (`rustc --print target-cpus` identifies the host as `apple-m4`),
plain release versus native release, `--benchmark --mib 32`, 120×40. Three
runs each, ordered A B B A A B, parser MB/s median (min–max):

| Workload | Portable release | Native release | Change |
|---|---:|---:|---:|
| log | 518.5 (518.4–523.1) | 524.9 (518.6–531.4) | +1.2% |
| compiler | 403.1 (340.6–419.4) | 407.4 (395.7–410.3) | +1.1% |
| ls | 451.6 (451.1–460.3) | 448.5 (447.9–459.5) | -0.7% |
| source | 396.4 (394.9–408.7) | 390.7 (388.3–393.7) | -1.4% |
| unicode | 231.8 (230.9–236.2) | 229.4 (229.3–232.4) | -1.0% |
| cjk | 305.7 (304.6–313.8) | 303.7 (300.5–308.9) | -0.7% |
| emoji | 223.2 (222.9–228.6) | 227.7 (222.3–228.3) | +2.0% |
| tui | 492.8 (488.3–523.2) | 496.9 (486.9–505.6) | +0.8% |
| editor | 511.7 (507.3–527.3) | 519.2 (503.4–519.5) | +1.5% |
| progress | 682.6 (679.2–685.4) | 693.0 (677.9–696.9) | +1.5% |
| truecolor | 747.9 (744.8–752.2) | 747.2 (746.0–758.8) | -0.1% |
| cursor | 463.7 (462.4–472.6) | 460.7 (446.3–460.9) | -0.6% |
| shell | 356.0 (351.3–368.3) | 357.4 (346.4–358.0) | +0.4% |
| sixel | 374.9 (373.2–383.0) | 373.7 (361.5–374.4) | -0.3% |
| kitty-rgba | 3884.8 (3825.6–3952.8) | 3948.0 (3822.9–3980.2) | +1.6% |
| kitty-png | 190.5 (190.2–193.7) | 194.7 (188.5–195.2) | +2.2% |
| kitty-zlib | 484.2 (483.7–491.8) | 496.0 (485.9–498.5) | +2.4% |
| overall | 392.9 (388.9–402.8) | 395.5 (390.1–397.1) | +0.7% |

Overall +0.7% is inside the run-to-run spread; this does not establish a
repeatable throughput improvement or a ranking against another terminal.
The native build option is provided for host-specific builds, not as a
measured performance claim.
