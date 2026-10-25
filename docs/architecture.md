# Architecture

Poopterm is one process with a UI thread plus three threads per tab (PTY
reader, parser, PTY writer). Everything is drawn on the CPU into a softbuffer
surface.

## Data flow

```
            ┌──────────────────────────── per tab ─────────────────────────────┐
 shell ─pty─┤ reader ──► OutputQueue (1 MiB) ──► parser thread ──► SharedTerm  │
            │                                     │ replies ──► PtyWriter ─────┼─► shell
            └─────────────────────────────────────┼──────────────────▲─────────┘
                                         wake (Output)                │ lock: snapshot / state
 winit events ──────────────────────────►  App (UI thread) ───────────┘
                                           ├─ input encoders ─► PtyWriter ─► shell
                                           └─ FramePacer ─► draw: Frame ─► damage ─► canvas ─► softbuffer
```

1. **Reading.** `pty::spawn_session` starts a reader thread that appends each
   read to the tab's `OutputQueue`. If parsing falls 1 MiB behind, the reader
   waits, so a flood pushes back on the producing program instead of using
   memory. At end of file it closes the queue.
2. **Parsing.** The tab's parser thread (`pty::pump`) takes everything
   queued and applies it to the `term::SharedTerm` (a `Mutex<Term>`) in
   64 KiB chunks, yielding to a waiting UI thread between chunks. Replies the
   terminal owes the program (cursor reports, device attributes) go straight
   to `PtyWriter`. After each batch it sends `UserEvent::Output(id)` unless
   one is still unacknowledged, and it records whether the batch was a
   backlog (half the queue or more): a *flood*. When the queue is closed and
   drained it sends `UserEvent::Closed(id)`, so a tab's last output is never
   lost.
3. **Reacting.** `App::process_output` (in `app/output.rs`) acknowledges the
   wake-up, then briefly locks the terminal for OSC 52 clipboard writes,
   bell/title/cwd changes (compared with `Session::seen`) and long-command
   notifications.
4. **Drawing.** Output-driven redraws go through `app::FramePacer`: at most
   one frame per monitor refresh, or 30 fps while the active tab floods.
   Input-driven redraws are requested immediately. Each frame copies the
   visible rows into a `term::Frame` under the lock and renders from that
   copy (see Rendering).

The UI thread must not lock a terminal twice: `std::sync::Mutex` would
deadlock, so debug builds track held locks and panic instead.

## Terminal state (`src/term`)

- `Term` holds the screen `Grid`, the scrollback, cursor, modes, palette,
  tab stops, image placements and stored kitty images, and
  shell-integration markers.
- `Grid` stores one `Vec<Cell>` per row. Scrolling rotates row headers (a
  line feed moves them up by one in a single `ptr::copy`) and copies the row
  leaving the screen into the scrollback, so a line feed costs O(columns),
  not O(screen). Only the written prefix of that row is blanked again.
- Each grid row also records how far it is used: every cell after that is
  the padding cell (default colors, blank). Rows entering the history are
  only scanned for trailing padding up to there, and an erase to the end
  of a line in default colors stops there and shortens the row. Printing
  marks the cells it writes; any other `IndexMut` access counts the whole
  row as used, so code that does not know about extents stays correct.
- `term::scrollback::Scrollback` is one ring buffer of cells holding rows
  without their trailing default blanks, so typical output takes a fraction
  of a full row; a row never wraps around the ring's end. On x86-64 rows are
  copied in with non-temporal stores, since nothing reads them soon;
  `TermGuard::drop` issues one `sfence` before unlocking, so the next lock
  holder sees them whole.
- Runs of cells are filled with 32-byte AVX2 stores when the CPU has them
  (`term/fill.rs`, checked once at runtime), and with `u64x2` otherwise.
- `Cell` is 16 bytes: character, foreground, background, attribute bits
  (`term::attr`), width (1 normal, 2 wide, 0 wide-character spacer) and an
  `extra` id. Combining marks, OSC 8 hyperlinks and OSC 66 blocks are interned in
  `term::extras::Extras`, shared copy-on-write with snapshots.
- **Synchronized output** (`?2026`): `begin_sync` freezes a `Frame` of the
  visible screen. Updates and replies apply to the terminal as usual, but
  `Term::snapshot` hands the renderer the frozen frame until the end marker
  or a 250 ms timeout, so it never shows a half-drawn screen. The copy is
  one screen, whatever the history holds.
- The alternate screen swaps in a fresh `Grid` and keeps the primary one in
  `primary_screen`.

## Tokenizing (`src/vt`)

`vt::Parser` is poopterm's own escape-sequence tokenizer: the DEC/ECMA-48
state machine, written from the specifications, with SIMD scans
(`vt/simd.rs`, `u8x32`) that find where bulk work ends. It calls a
`vt::Handler`:

- **Text in runs.** Ground state scans to the next C0 control in one pass.
  Pure printable ASCII goes to `print_ascii(&[u8])`; anything else is
  validated once and goes to `print(&str)`, with C1 controls
  (U+0080–U+009F) split out and executed. `vt/utf8.rs` validates with the
  Keiser–Lemire nibble-table algorithm in AVX2 when the CPU has it, and
  `str::from_utf8` elsewhere; only an invalid run takes the per-defect path
  that inserts U+FFFD.
- **Payloads in slices.** OSC fields, DCS payloads (`put(&[u8])`, Sixel) and
  APC strings (`apc_start` / `apc_put` / `apc_end`, the kitty graphics
  protocol) are scanned in bulk, not byte by byte. OSC strings over 4 MiB
  are dropped.
- **CSI parameters in a tight loop** (`sequence.rs`), entered straight from
  ground state on `ESC [`. Anything unusual falls back to the general
  per-byte state machine, so both paths always agree.
- **Split reads.** A UTF-8 character split across reads is completed from
  the next one; sequences carry their state across reads.

Semantics match vte 0.15, which stays as a dev-dependency: `vt/tests.rs`
feeds random streams through both and requires identical events, with our
side split into random reads (and byte-by-byte). Known differences on
purpose: APC strings are delivered (vte drops them), and characters split
across reads are not corrupted (vte 0.15 drops bytes after them).

## Applying sequences (`src/parser`)

`parser::Parser` wraps `vt::Parser` with `Term` as the handler. The
per-family handlers (`csi.rs`, `esc.rs`, `osc.rs`, `sgr.rs`, `print.rs`)
are `impl Term` blocks.

Printing non-ASCII text (`print.rs`) avoids per-character work where it can:

- `narrow.rs` prints runs of narrow one- to three-byte characters (Latin,
  Greek, Cyrillic, punctuation, box drawing) sixteen bytes at a time,
  decoding them with SIMD instead of one by one.
- Widths come from `src/width.rs`, a two-level table (`src/width.bin`,
  13.5 KiB) that `scripts/width_table.py` generates from the Unicode data
  files; `unicode-width` is only the test oracle.
- Cell words for narrow and wide characters are built once per segment.
- Combining marks and joined characters (`cluster.rs`) attach to the cell
  just written without ending the run, through a cold out-of-line helper.

`batched_printing_matches_per_character_printing`
feeds random streams through `parser::Parser` and through vte driving
`Term` one character at a time (`parser::PerCharacter`), and requires
identical terminal state.

Other families have modules of their own, each an `impl Term` that parses
into bounded state in `src/term`:

| Sequences | Parsing | State |
|-----------|---------|-------|
| Colors (OSC 4–6, 10–21, 104–119, color stack) | `osc_color.rs`, `color_spec.rs` | `term::colors` |
| Clipboard (OSC 52, 5522) | `osc_clipboard.rs` | `term::clipboard`, written by the UI thread |
| Notifications (OSC 9, 777, 99) | `osc_notify.rs` | `term::notifications`, shown by `app/notifications.rs` |
| Program status (OSC 7501, 9;4) | `osc_status.rs` | `term::status` |
| Pointer shapes (OSC 22) | `osc_pointer.rs` | `term::pointer` |
| Shell integration (OSC 133, 633, ConEmu 9;n, 3008) | `osc.rs`, `osc_shell.rs` | `shell_integration::markers`, `term::contexts` |
| iTerm2 (OSC 1337, 6;1, 21337) | `osc_iterm.rs`, `osc_tab.rs` | `term::iterm`, `term::iterm_images`, `term::tab_style`, `graphics::iterm` |
| Text sizing (OSC 66) | `osc_text_size.rs` | `term::multicell`, drawn by `render::multicell` |
| DECRQSS, XTGETTCAP (DCS) | `dcs.rs` | `term::dcs`; capabilities from `terminfo_caps` |
| XTWINOPS (`CSI t`) | `window_ops.rs` | title stacks on `Term` |

Requests the UI thread must act on (clipboard writes, notifications, focus
and attention) are queued on `Term` and taken by `App::process_output`.
Replies go to `Term::responses`, moved out of a synchronized-output
snapshot at once so they are never held back.

**Scaled text.** Every cell of an OSC 66 block holds its own copy of the
block (text, size, offset in the block) in `term::extras`, so a row draws,
scrolls and copies alone. While `Term::has_multicells` is set, printing
takes the general path, which applies the protocol's overwriting rules,
and erasing, inserting and deleting erase the blocks they would cut.

## Desktop integration (`src/dbus`, `src/notifier`)

- `dbus` is a D-Bus client written from the specification: the wire format
  (`wire.rs`) in either byte order with the specification's limits, messages,
  and a connection doing SASL EXTERNAL. Tests run against a private
  `dbus-daemon` (`test_bus.rs`).
- `notifier` delivers notifications through the freedesktop.org service, or
  `osascript` on macOS; it rate limits each session (`rate.rs`) and maps
  desktop notification ids back to the program that asked (`routes.rs`).

## Images (`src/graphics`, `src/sixel.rs`, `src/term/kitty*.rs`)

Both image protocols end in `graphics::Placement`s on `Term::images` (the
alternate screen keeps the primary screen's in `primary_images`): an
`Arc<Bitmap>` of straight-alpha `0xAARRGGBB` pixels, a grid position (the
row goes negative once scrolled into history), a source rectangle, an
extent, a z-index and, for kitty images, the image and placement ids.
`Term::scroll_images` moves placements with the text. Sixel images are
erased by text written over them; kitty images only by delete commands,
`ED 2`/`ED 3`, RIS and leaving the alternate screen.

The kitty graphics protocol runs on the parser thread:

- `vt` hands APC strings over in slices; `kitty::Receiver` checks the `G`,
  parses the control keys (`kitty::Command`), decodes base64 as it arrives
  (`graphics::base64`, SIMD for whole blocks) and joins chunked transfers.
  Limits are enforced while receiving, before anything is decoded.
- `kitty::load` reads file, temporary-file and shared-memory media;
  `kitty::decode` turns RGB, RGBA and PNG (optionally zlib) into a `Bitmap`.
  Zlib payloads (`o=z`) go through `crate::inflate`, limited to the exact
  image size; PNG still uses the `image` crate.
- `kitty::Store` keeps images by id and number with a quota and least
  recently used eviction that prefers images not on screen. Each image is an
  `Animation`: its frames, the shown frame and the playback timeline.
  Virtual placements (`U=1`) live there too.
- `term/kitty.rs` applies transmit, place and delete; `term/kitty_animation.rs`
  applies frames, composition and playback. Replies go to `Term::responses`.
- Running animations are advanced by the UI thread (`about_to_wait`), which
  sleeps until the next frame of an animation that is on screen.
- Unicode placeholder cells (U+10EEEE) store the image id as the client set
  the foreground color (`Term::fg_id`). `Term::snapshot` resolves them into
  `Frame::placeholders`, and only while virtual placements exist.

Sixel (`src/sixel.rs`) is decoded with explicit size limits into one flat
buffer of final `0xAARRGGBB` pixels: colors get their alpha when they are
chosen, so the buffer becomes the bitmap as is.

## Inflate (`src/inflate`)

Our own zlib/DEFLATE decoder (RFC 1950/1951), written from the
specifications, with no `unsafe`:

- `bits.rs` reads 64 bits at a time and refills eight bytes at once.
- `huffman.rs` builds two-level tables (10-bit primary for literals and
  lengths, 8-bit for distances) whose entries hold the kind, value, extra
  bits and code length; over-subscribed and incomplete codes are refused.
- `mod.rs` decodes stored, fixed and dynamic blocks. While enough room
  remains, a fast loop keeps the reader in registers, decodes up to three
  literals per refill and copies matches with `u8x16` (whole vectors, a
  splat for distance 1, a `swizzle_dyn` pattern for other short periods);
  near the end a careful per-symbol path takes over.
- `adler.rs` checks Adler-32 with `u32x32` lanes.

The output never grows past the caller's limit, and a match that would end
past it is refused. `miniz_oxide` is the test oracle and compresses the
benchmark corpus.

The renderer draws each row in layers: images below `i32::MIN / 2`, cell
backgrounds, other negative z-indices, text and placeholder slices, then
z-index 0 and above. Images are clipped per row, so damage tracking can
redraw single rows; `render::damage` compares placements by bitmap identity
and position, and placeholder slices row by row.

## Rendering (`src/render`, `src/app/draw*.rs`)

- `render::painter::Painter` wraps the frame buffer with clipped `rect`,
  `round_rect`, `button`, `text` and `glyph` operations.
- `render::glyphs::FontSet` owns the primary and fallback fonts and caches
  rasterized glyphs by `(char, size)` in an `FxHashMap`; the fallback font is
  resolved only on a cache miss. When no loaded font has a character,
  `fonts::fallback::SystemFallback` opens the font database (each face records
  the Unicode pages its `cmap` maps) and loads matching installed fonts, at
  most four per character and 32 in all.
  Emoji-presentation characters prefer a font with a color bitmap
  (`fonts::cbdt` reads `CBLC`/`CBDT`, `fonts::color` scales the PNG to the
  font size); `Glyph::color` holds those pixels, which the painter blends
  in their own colors, unslanted by italic.
- `render::grid::draw_grid` draws a `GridView`: cells right to left (so wide
  glyphs paint over their spacer), cursor, selection, search match, underline
  and strike styles, hyperlinks, combining marks and images.
- `render::blend` blends glyph coverage four pixels at a time with
  `portable_simd`.
- `app/draw.rs` composes a full frame: background or wallpaper, tab bar
  (`draw_tabs.rs`), terminal grid, search box, command palette, status bar.
  The Settings window is drawn by `app/settings_view.rs`.

**Damage tracking.** `app::canvas::Canvas` keeps a back buffer holding the
last frame and the `Frame` it shows. `render::damage::grid_damage` compares
the new `Frame` with it row by row (cells, marks, links, cursor).
When lines scrolled into history (`Frame::top_line` grew), the old pixels are
shifted up and only the new rows are drawn; with a wallpaper, rows are
redrawn instead, because the wallpaper does not scroll. `app/repaint.rs`
restores the background under damaged rows, redraws them and the tab and
status bars, and copies only changed pixel rows to the window buffer. A full
frame is drawn after resizes, theme or font changes, on tab switches, focus
changes, and while a selection, search match or overlay is shown. Glyphs are
clipped to their row so rows can be redrawn independently. X11 windows get
the whole image each present (they lose contents when exposed); Wayland and
macOS get damage rectangles.

## Input (`src/input`)

All encoders are pure functions with unit tests:

- `keys::encode_key(key, modifiers, modes)`: legacy xterm and kitty
  (`CSI u`) encodings, Alt as ESC prefix, modified cursor and function keys,
  DECCKM application mode.
- `mouse::encode_mouse`: SGR (`?1006`) and X10 reports with modifier bits.
- `paste::paste_bytes`: CR line endings; strips paste markers from pasted
  text when bracketed paste is on.
- `scroll::ScrollAccumulator`: touchpad pixel deltas to whole lines;
  `alternate_scroll_bytes` for wheel scrolling in alternate-screen programs.
- `shortcuts::Shortcuts`: configurable chords, parsed once from the config.

## Configuration and persistence

- `config.rs`: a small TOML-like `key = value` format. Saving rewrites only
  the changed keys and keeps unknown keys and comments.
- `session_store.rs`: tab working directories in `sessions.txt` (hex-encoded
  paths) next to the config file.
- `theme.rs`: built-in palettes and the xterm 256-color cube.
