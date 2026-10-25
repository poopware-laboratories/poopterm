# TODO

Open work, roughly in priority order. Measure before and after every step
(`docs/performance.md`); drop changes without a reproducible gain.

## Throughput goals

Everything should be as fast as we can make it while staying stable and
correct. The numbers below are minimums, not finish lines.

**Text: at least 200 MB/s end to end on every payload** (ASCII, Unicode,
CSI-heavy, cursor movement). 200 MB/s is the floor, not the ceiling: go
higher wherever that stays safe (bounded memory, no skipped state, output
identical to the slower path). Measured with `scripts/bench-throughput.py` in a real
window. Headless on the i3-N305 (26.10.42) the workloads run at 113–1023
MB/s (175 overall); emoji, Unicode, source, shell prompts, cursor motion,
sixel, kitty PNG, compiler output, CJK and ls are still below the floor
there, and end to end everything is lower. See [`docs/performance.md`](docs/performance.md).

**Images: as fast as possible** for Sixel and the kitty graphics protocol,
from first byte to pixels on screen. There is no target to stop at; at least
match text throughput, within the explicit size limits on untrusted input.

- **Image benchmark.** Sixel and kitty payloads (raw RGBA, PNG, zlib) are
  in the corpus, so `--benchmark` and `bench-throughput.py` cover them. Still
  missing: full-screen frames and animation, reported in frames per second.
- **Sixel decoding.** Now decoded straight into one flat buffer of final
  pixels (~150 MB/s headless); remaining candidates are SIMD band decoding
  and streaming decode instead of buffering the whole DCS.
- **Kitty image decoding.** Raw RGBA parses at ~1 GB/s and zlib through our
  own inflate at ~246 MB/s; PNG at ~113 MB/s spends ~60% inflating and ~20%
  unfiltering inside the PNG crate. Our own PNG decoder (our inflate plus a
  SIMD unfilter) is the next step; the RGBA path could decode base64
  straight into pixels instead of through a byte buffer.
- **Rendering during floods:** frames still write unchanged background pixels,
  competing with parsing for memory bandwidth. Evaluate skipping those pixels
  or lowering internal resolution, retaining a change only with repeatable
  throughput and visual gains.
- **Reduce copies in PTY input:** an owned-buffer queue prototype did not show
  a repeatable gain across payloads and was removed. Revisit only with a
  benchmark that isolates copy cost while preserving queue backpressure.
- **Fair comparisons.** Compare terminals at the same window size and in the
  same session; the benchmark now names terminals by XTVERSION and no longer
  aborts in kitty.

## Packaging

Every package installs through `scripts/install.sh` (desktop entry, icons,
terminfo) and is built, installed and run in CI. Releases bump versions in
packaging files through `scripts/release.py`, in the release commit.

- **Gentoo ebuild.** `x11-terms/poopterm` in an overlay (in this repository
  or a separate `poopterm-overlay`), built from the release tag. Cargo
  ebuilds list every crate in `CRATES`; generate that from `Cargo.lock`
  with our own script, have `release.py` rename the ebuild per release,
  and build and install it in a `gentoo/stage3` CI job. The nightly
  toolchain is the hard part: Portage's Rust is stable only.
- **Nix flake.** A `flake.nix` with a `packages.default` built by
  `rustPlatform.buildRustPackage` from `Cargo.lock` (no vendor hash to
  bump), a nightly toolchain from a pinned overlay, the display libraries
  winit loads added to the runtime path (`patchelf --add-rpath`, since
  NixOS has no global library path), and a `devShell`. CI runs
  `nix build` and `nix flake check`; a scheduled job updates `flake.lock`
  through a pull request.
- **AUR.** Publish `packaging/arch/PKGBUILD` (with a generated `.SRCINFO`)
  to the AUR from the release job, with its own deploy key.

## Build and layout optimizations

Heavier, stable optimizations of the shipped binary and of our data layout.
Each item is an issue; measure every one before and after, and keep it only
with a reproducible gain. The goal is heavily optimized binaries in **every
release artifact** (deb, rpm, AppImage, macOS and whatever comes next). The
packaging side of that is vgs499p's area.

- **Profile-guided and post-link optimization**
  - A staged PGO + BOLT pipeline producing the binary in every release
    artifact ([#34](https://github.com/poopterm-labs/poopterm/issues/34)).
  - Sample-based PGO (AutoFDO) from perf profiles of real use, with nightly
    `-Zprofile-sample-use` ([#35](https://github.com/poopterm-labs/poopterm/issues/35)).
- **Code generation**
  - Runtime CPU dispatch (AVX2, AVX-512) for the SIMD hot paths, keeping a
    baseline x86-64 binary ([#36](https://github.com/poopterm-labs/poopterm/issues/36)).
  - `-Zbuild-std`, so LTO, PGO and `panic = "abort"` cover std too
    ([#37](https://github.com/poopterm-labs/poopterm/issues/37)).
  - Hot-function ordering and identical code folding at link time, which
    also works on macOS ([#38](https://github.com/poopterm-labs/poopterm/issues/38)).
- **Memory layout**
  - Split `Term` into hot and cold state, with the per-byte parser state on
    one or two cache lines ([#39](https://github.com/poopterm-labs/poopterm/issues/39)).
  - Store grid and scrollback rows contiguously, not as one `Vec` per row
    ([#40](https://github.com/poopterm-labs/poopterm/issues/40)).
  - Shrink `Cell` below 16 bytes ([#41](https://github.com/poopterm-labs/poopterm/issues/41)).
  - Cache-line alignment of the structures shared between threads, plus
    prefetching where profiles show misses
    ([#42](https://github.com/poopterm-labs/poopterm/issues/42)).
- **Regression to fix first.** The compatibility work from 26.10.23 to
  26.10.27 cost 2–4% on escape-sequence-heavy workloads in the parser
  benchmark (tui, editor, cursor, progress) and about 6% on unicode. Profile
  and recover it before or alongside the items above.

## Hardware-accelerated rendering

Full GPU rendering, now that kitty graphics has landed. CPU drawing is the main
latency cost: a full redraw takes 25.6 ms at 1000×600, and much longer at
HiDPI sizes. It also takes memory bandwidth from the parser during floods,
which is why floods are capped at 30 fps. Parse throughput stays CPU work.

Tracked in [#30](https://github.com/poopterm-labs/poopterm/issues/30), with
one issue per piece (#20–#29); claim one there.

- **Strict rule: no `wgpu`, WebGPU, OpenGL or binding crates.** Write our
  own renderers on our own thin FFI: Vulkan on Linux and FreeBSD (loaded
  with `dlopen`), Metal on macOS (through the Objective-C runtime).
- **Software rendering stays as the fallback**, used when the GPU path fails
  to start (VMs, X forwarding, broken or missing drivers). It is also the
  reference: GPU output must match it in the pixel-equivalence tests.
- **Design:** a small `Renderer` boundary (a `term::Frame` and damage go in,
  a presented frame comes out), so `term` and `parser` never see the
  backend. Expected pieces: a glyph atlas with partial uploads, instanced
  cell quads for backgrounds, cursor and selection, and image textures for
  Sixel and kitty placements (GPU scaling and blending). The 30 fps flood
  cap goes away if measurements show parsing no longer suffers.
- Measure frame time, input latency and flood throughput before and after
  on every platform, as for any performance work.

## Resource management and memory

- **Memory footprint.** Reported at about 400 MB on macOS, staying near
  140 MB afterwards. Measured on Linux (26.10.55, one tab, 20,000 lines of
  scrollback): 58 MB idle, 130–230 MB after a flood that fills the history,
  depending on window width. Repeated floods and clears do not raise it
  further, so no leak has been found; the memory is retained, not lost.
  Where it goes, worst first:
  - *Fonts are read whole onto the heap.* Up to eight symbol fallback fonts
    of up to 64 MB each (`fonts::load`) plus the main font are copied with
    `fs::read`. macOS system fonts are large (PingFang, Hiragino, Apple
    Color Emoji), which likely explains most of the macOS figure. Map font
    files read-only (`mmap`; `Font` already takes a `Cow<[u8]>`) so they
    cost shared, evictable page cache instead of private memory, and load
    fallbacks only when a missing glyph needs them.
  - *The scrollback ring grows by doubling and copying* (`Scrollback::grow`):
    during each step the old and new rings are both live, and the capacity
    is kept after `ESC [3J` or a lower line limit. Grow in fixed-size
    segments without copying (fits #40), and release the capacity on clear
    and when the limit shrinks. It costs about 16 bytes per cell of history,
    per tab; shrinking `Cell` (#41) cuts all of it.
  - *mimalloc keeps freed pages committed.* With `MIMALLOC_PURGE_DELAY=0`
    the same flood ends at 83 MB instead of 134 MB. Set purge options at
    startup, or allocate the few huge buffers (scrollback, frame buffer,
    image bitmaps) with `mmap` directly so freeing returns them at once.
    Measure throughput, because eager purging costs page faults.
  - *Idle baseline.* 35 MB of heap before any output: audit it (fonts, font
    database, glyph caches, per-tab buffers sized for the largest window).
  - General Rust techniques to apply where profiles point: `shrink_to_fit`
    or dropping buffers after spikes (PTY queue, OSC/DCS payloads, kitty
    transfers), `Box<[T]>` over `Vec<T>` for fixed data, smaller
    enums and indices (`u32`/`u16` over `usize`), interning repeated
    strings (hyperlinks, titles), and lazy initialization of rarely used
    state (`Option<Box<…>>`, `OnceCell`).
  - Track it: report peak and settled RSS in the benchmark, and test that
    clearing history releases its memory. Check on macOS (`footprint`,
    `vmmap`) and Linux (`/proc/<pid>/smaps_rollup`).
- **cgroups v2 per tab, fully automatic (Linux).** Like Ghostty, put
  each shell in its own cgroup, so a runaway program in one tab cannot
  starve the terminal or the other tabs. The terminal process keeps its
  own group. No configuration: at startup, check that `/sys/fs/cgroup` is
  a cgroup2 mount (`cgroup.controllers` exists) and that we may create
  groups there. Do that either through a delegated subtree we can write
  (our own cgroup's directory) or by asking systemd for a transient user
  scope over D-Bus. If neither works, run exactly as today with no
  errors. Start with per-tab groups (memory and pids accounting, a
  `memory.high` so one tab is throttled before the OOM killer picks the
  terminal), then consider CPU weight so the UI and parser threads stay
  responsive while a tab floods. Test in CI: available, read-only and
  missing cgroupfs all behave correctly.
- **Transparent huge pages for large buffers.** Large, long-lived
  allocations (scrollback rows, image bitmaps, the frame buffer, PTY
  queues) could cause fewer TLB misses on huge pages. Check once at
  startup by reading `/sys/kernel/mm/transparent_hugepage/enabled`
  (`always` or `madvise`, cached). Then `madvise(MADV_HUGEPAGE)` only
  allocations of at least 2 MiB, aligned so they can actually use huge
  pages. FreeBSD promotes superpages by itself, and macOS has no
  equivalent, so it is Linux-only and does nothing elsewhere. Measure RSS
  and throughput before and after, and keep it only for buffers where it
  helps reproducibly.

## Replacing dependencies with our own code

Chosen to replace with our own implementations, built around this
project's needs and optimizations. Each replacement is tested against the
crate it replaces as an oracle (differential tests, and fuzzing for parsers
of untrusted input), and lands only once it matches it and measures at
least as fast.

- **`portable-pty` → our own PTY layer** (libc: `posix_openpt`/`openpty`,
  `fork` + `setsid` + `TIOCSCTTY`, `exec`, `TIOCSWINSZ`, non-blocking fds).
  It gives us control of read sizes and polling, and drops
  Windows-oriented abstractions we do not use. Must pass CI on Linux, macOS
  and FreeBSD.
- **`unicode-width` → generated width tables:** done. `scripts/width_table.py`
  turns the Unicode data files into a two-level table (`src/width.bin`,
  13.5 KiB) that `src/width.rs` looks up; an exhaustive test checks every
  code point against the crate, now only a dev-dependency. Regenerate the
  table for each Unicode release (the script's docstring says how) and bump
  the oracle with it.
- **`miniz_oxide` → our own inflate (RFC 1950/1951):** done for kitty
  `o=z` (`src/inflate`: no `unsafe`, output bounded by a limit before it
  grows, two-level Huffman tables, a fast loop that decodes up to three
  literals per refill, `u8x16` match copies, SIMD Adler-32). Randomized and
  damaged streams are tested against `miniz_oxide`, which stays only to
  compress the benchmark corpus and as the test oracle;
  `inflate_throughput` compares the two. Still to do: PNG, which needs our
  own PNG decoder (unfiltering) in place of the `image` crate.
- **`fontdue` → our own rasterizer:** done for drawing (`src/fonts`: cmap,
  glyf, CFF/CFF2, a SIMD coverage rasterizer), with fontdue kept as the
  test oracle (`font_tests.rs`). Loading is lazy, so startup no longer
  parses every glyph of every fallback font (about 400 ms on a desktop
  with Nerd Fonts). Still to do: per-glyph rasterization, which is slower
  than fontdue's precomputed outlines (`font_engine_speed`); kerning;
  feeding the GPU glyph atlas. Color emoji (COLR/CBDT/sbix) is a separate
  step.
- **`softbuffer` → our own present path** (X11 MIT-SHM, Wayland `wl_shm`,
  macOS CALayer) for the software renderer, which becomes the fallback
  beside Vulkan/Metal. Do it together with the GPU work so presentation and
  damage handling are designed once.

Kept for now: `winit` (windowing, IME and DPI are too much risk for the
gain), `image` (wallpapers and iTerm2 inline images need JPEG and WebP), and `rfd`.

## Protocol gaps

- **GIF inline images.** iTerm2 inline images accept PNG, JPEG and WebP;
  GIF (often animated) needs our own decoder, since enabling the `image`
  crate's GIF support would add a dependency. Animated GIFs can reuse the
  kitty animation frames.
- **OSC 176 (application id)** is ignored: a Wayland app id is fixed once
  the window exists, so it cannot follow a program's request.

## UI overhaul

- **Center the chrome.** Many items sit off center: tab titles ride high in
  their tab, the tab bar's labels, buttons and status bar text do not share
  a baseline, and the + and settings buttons are not centered on the bar.
  Derive every vertical position from one function of the bar height and
  the font's real ascent and descent, and pin it with pixel tests.
- **Drop the "poopterm" label** left of the tabs on Linux; the tabs start
  at the window edge (after the window buttons where those sit on the left).
- **One title line on Linux.** Merge the title bar into the tab bar: tabs,
  new tab, settings, then minimize, maximize and close, in one row, with
  the empty part of the row dragging the window (`drag_window`), a double
  click maximizing, and the edges resizing it (`drag_resize_window`). This
  means drawing our own decorations (`with_decorations(false)`) where the
  compositor offers client-side ones (GNOME, KDE on request); keep server
  side decorations where the user or compositor asks for them, and draw no
  buttons on tiling compositors such as Sway. macOS already puts the tabs
  in the title bar beside the system's traffic lights.

## Accessibility

The gaps listed in [`ACCESSIBILITY.md`](ACCESSIBILITY.md); keep the two in
step.

- **Screen readers.** Expose the grid as an accessibility tree, written
  in-house from the specifications: AT-SPI over our own D-Bus code
  (`src/dbus`) on Linux and FreeBSD, NSAccessibility on macOS. Text, cursor
  position and new output announcements first; selection after.
- **IME preedit.** Draw the composing text at the cursor and place the
  candidate window there (`Ime::Preedit`, `set_ime_cursor_area`).
- **Keyboard-only Settings.** Tab and Shift+Tab focus order through every
  control, with a visible focus ring.
- **Reduced motion.** A setting that pauses kitty animations, following the
  desktop's reduced-motion preference where the platform exposes one.
