# Testing

Poopterm is developed test-first, with at least three lines of tests for
every line of production code in each pull request (see the workflow in
[`AGENTS.md`](../AGENTS.md#workflow-31-test-driven-development)). This page
covers where tests live and how to write them; the 3:1 check counts the
places listed under [Where tests go](#where-tests-go) as tests.

## Running

```sh
cargo test                       # everything except benchmarks; no window needed
cargo test term::                # only terminal-emulation tests
cargo test -- --nocapture name   # one test with its output
cargo test --release -- --ignored --nocapture parser_throughput   # benchmark
```

Benchmarks are ignored tests, run in release mode with `--ignored
--nocapture` and their name:

| Benchmark | Measures |
|-----------|----------|
| `parser_throughput` (`src/bench`) | Parser and terminal on the benchmark corpus. |
| `inflate_throughput` (`src/inflate/throughput_bench.rs`) | Our inflate against `miniz_oxide` on image-like and synthetic streams. |
| `font_engine_speed`, `font_engine_stages` (`src/fonts/font_bench.rs`) | Our rasterizer against fontdue, whole and per stage. |
| `highlight_rendering_throughput` (`src/app/canvas_pixels_tests.rs`) | Full against partial redraws. |

`installed_fonts_match_fontdue` is ignored too: it compares every font
installed on the machine, which is slow and differs between machines.

Tests that need an external program (the shell-integration tests run `zsh`
and `bash`, the D-Bus tests `dbus-daemon`) return early when it is missing, so they pass on minimal CI
images and run fully on developer machines.

## Where tests go

Unit tests sit at the bottom of the file they test, in a `#[cfg(test)] mod
tests`. Once they outgrow that (files stay under about 400 lines), they move
to a sibling `name_tests.rs`, or several split by topic, declared at the end
of the module as `#[cfg(test)] mod name_tests;`. The larger suites:

| Suite | What it covers |
|-------|----------------|
| `src/term/tests.rs` | Emulation: bytes in, grid/cursor/modes/replies out. Includes the randomized bounds test and the parser differential test. |
| `src/term/kitty_tests.rs`, `src/term/kitty_animation_tests.rs` | Kitty graphics emulation: commands in, placements, stored images, frames, cursor and replies out. Their helpers (`terminal`, `send`, `replies`) give 10×20 pixel cells. |
| `src/app/canvas_pixels_tests.rs` | Pixel tests: partial redraws equal full redraws (highlights, scrolling, images, placeholders, animation frames), and image layering. |
| `src/input/*` | Exact byte sequences for keys, mouse reports, paste and scrolling. |
| `src/app/settings.rs`, `src/ui/*` | Hit-testing and layout as pure functions. |
| `src/pty/*` | Output queue backpressure and the non-blocking writer (real pipes and threads). |
| `src/shell_integration.rs` | The shipped zsh/bash hooks, run in real shells. |
| `src/vt/tests.rs`, `src/vt/utf8_tests.rs` | Tokenizer events against vte; the UTF-8 validator's tables against a scalar model and every variant against `str::from_utf8`. |
| `src/parser/print_tests.rs`, `narrow_tests.rs`, `cluster_tests.rs` | The printing fast paths against per-character printing. |
| `src/term/scrollback_tests.rs`, `history_tests.rs`, `fill_tests.rs`, `width_tests.rs` | The scrollback ring; how trimmed history rows are shown, searched and copied; AVX2 fills against the portable fill; widths. |
| `src/sixel_tests.rs`, `src/sixel_reference_tests.rs` | Sixel decoding and limits, and the flat decoder against the original per-row decoder kept as its oracle. |
| `src/inflate/*_tests.rs` | Inflate against `miniz_oxide`: round trips, every length and distance, malformed and damaged streams, limits. |
| `src/fonts/*_tests.rs` | Font parsing and rasterizing against fontdue; malformed fonts. |
| `src/dbus/*_tests.rs` | The D-Bus wire format and a private `dbus-daemon`. |

## Oracles and mutation checks

Code that replaces a crate or a slow path is tested against it: vte for the
tokenizer, `unicode-width` for widths, `miniz_oxide` for inflate, fontdue
for fonts, `str::from_utf8` for UTF-8 validation, the per-character printer
for the fast printing paths, and the portable code for every SIMD or
CPU-specific kernel. Random inputs (from the seeded `bench::corpus::Rng`, so
failures reproduce) find the cases nobody thought of; handwritten cases pin
the edges (vector widths, buffer ends, limits).

A test that passes the first time proves nothing until it has been seen to
fail. For code whose tests were written after it (characterization, or a
test added to meet the 3:1 rule), break the code on purpose (flip a
comparison, drop a check, shift a bound by one) and run the tests: each
such mutant must make one fail. A mutant no test catches is either a
missing test or code that does nothing observable (often a performance-only
guard), which is worth a comment in the code.

## Writing emulation tests

Use the helpers at the top of `src/term/tests.rs`:

```rust
#[test]
fn erase_line_keeps_the_current_background() {
	let mut t = terminal(4, 2);           // 4 columns × 2 rows
	feed(&mut t, b"abcd\x1b[44m\r\x1b[K");
	assert_eq!(row(&t, 0), "    ");       // text of row 0
	assert_eq!(cell(&t, 0).bg, t.palette[4]);
}
```

- `feed` runs bytes through the real `Parser`.
- `row(&t, y)` returns a row's characters; `cell(&t, i)` returns the cell at
  flat index `y * cols + x`.
- Terminal replies accumulate in `t.responses`.
- Name tests after the behavior (`no_wrap_overwrites_last_column`), not the
  function.

When fixing a parser bug, add the triggering sequence to the fragments in
`vt/tests.rs` (tokenizer events against vte) and, when it affects terminal
state, to `batched_printing_matches_per_character_printing`. A failing
`vt/tests.rs` case is shrunk to a minimal stream automatically.

## Pure functions over UI state

Code that needs a window is hard to test, so keep decisions in functions
that take plain data and return plain data: `encode_key`, `encode_mouse`,
`settings_target`, `tab_layout`, `FramePacer::request`. `App` methods then
only gather inputs and apply results.

## Visual checks

`src/app/canvas_pixels_tests.rs` checks that partial redraws match full redraws
pixel for pixel, and a few exact pixels (image layering, placeholder
fitting). It cannot judge whether something looks right, so when changing
drawing code, also compare a fixed screen before and after:

1. Write a fake shell that prints a deterministic sample and sleeps (styled
   text, 256 colors, wide and combining characters, box drawing).
2. Launch both builds with it, using a temporary config so your own settings
   and sessions are untouched:
   `POOPTERM_CONFIG=/tmp/cfg/config.toml SHELL=/path/fake-shell.sh target/release/poopterm`
3. Screenshot both windows (on X11, `import -window <id> out.png`) and compare
   with ImageMagick: `compare -metric AE before.png after.png diff.png`.
   Zero differing pixels means the change is visually neutral.

Notes:

- Let the fake shell `sleep 1` before printing. Output is parsed as soon as it
  arrives, so a shell that prints before the window has its final size lays
  text out at the initial 80 columns, and the screenshot depends on timing.
- **Damage tracking.** Most frames redraw only changed rows. To check partial
  redraws, make the fake shell print in steps (scrolling, cursor moves), screenshot the same build with and without
  `POOPTERM_FULL_REDRAW=1`, and compare. That variable disables partial
  redraws.
- Where screenshots are unavailable (Wayland without X access), render the
  same bytes headlessly instead: a temporary ignored test that feeds them to
  a `Term`, draws the `Frame` with the helpers in `canvas_pixels_tests.rs` and
  saves a PNG with `image::save_buffer`. Remove the test afterwards.
- Debug builds panic if a thread locks a terminal it already holds, so run
  new UI code paths in a debug build at least once.
