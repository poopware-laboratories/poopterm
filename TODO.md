# TODO

Open work, roughly in priority order. Measure before and after every step
(`docs/performance.md`); drop changes without a reproducible gain.

## Throughput goals

Everything should be as fast as we can make it while staying stable and
correct. The numbers below are minimums, not finish lines.

**Text: 500 MB/s on every payload, or the machine's ceiling where that is
lower.** 200 MB/s stays the floor everywhere. The ceiling is measured, not
guessed: `poopterm --benchmark --stages ceiling,parse` runs, next to the
parser, the memory traffic no terminal can avoid for each workload (every
byte read once, one 16-byte cell written per character, the used cells of
each scrolled line copied into history; see
[`docs/performance.md`](docs/performance.md)). Run it on any device to get
that device's goal: `parse` should reach the lower of 500 MB/s and
`ceiling`, in safe code, with output identical to the slower path.

On the i3-N305 laptop (26.11.77 with history copied in safe code, 120x40,
median of 5, MB/s):

| workload | ceiling | parse | goal | parse / goal |
|---|---|---|---|---|
| log | 358 | 328 | 358* | 92% |
| compiler | 722 | 246 | 500 | 49% |
| ls | 431 | 283 | 431 | 66% |
| source | 939 | 212 | 500 | 42% |
| unicode | 493 | 175 | 493 | 35% |
| cjk | 749 | 198 | 500 | 40% |
| emoji | 837 | 168 | 500 | 34% |
| tui | 434 | 338 | 434 | 78% |
| editor | 574 | 344 | 500 | 69% |
| progress | 2709 | 416 | 500 | 83% |
| truecolor | 6162 | 319 | 500 | 64% |
| cursor | 1759 | 232 | 500 | 46% |
| shell | 824 | 202 | 500 | 40% |

\* On log the ceiling is memory bandwidth: one 16-byte history cell per
input byte. Plain stores into the history ring read each cache line from
memory before overwriting it, so history copies run at about 6.6 GB/s here
however they are batched below glibc's ~4 MiB non-temporal threshold. The
safe way past it is fewer bytes per history cell (see below). Emoji,
Unicode, shell, source, CJK and cursor are furthest from their goals.

- **Compact history rows.** Rows scrolled into history are copied as
  16-byte cells, which costs log, ls and every flood that scrolls (log
  −19%, ls −17% when the non-temporal stores went). Store history rows
  in a smaller format (characters plus runs of one style) and expand
  them when read, so the copy moves a fraction of the bytes; the
  `ceiling` model then needs the same format.

End to end in a window, the kernel's cooked PTY caps text at 67–118 MB/s
on this laptop, and Poopterm comes within 10% of that ceiling on 13 of 17
workloads; raw, the window ran at 119–478 MB/s (206 overall) at 26.11.51.
`poopterm --benchmark --terminal` measures it; the PTY ceiling there is the
kernel's, so headless `parse` against `ceiling` is where the 500 MB/s goal
is tracked. See [`docs/performance.md`](docs/performance.md#results).

**Images: as fast as possible** for Sixel and the kitty graphics protocol,
from first byte to pixels on screen. There is no target to stop at; at least
match text throughput, within the explicit size limits on untrusted input.

- **Image benchmark.** Sixel and kitty payloads (raw RGBA, PNG, zlib) are
  in the corpus, so `--benchmark` and `--benchmark --terminal` cover them. Still
  missing: full-screen frames and animation, reported in frames per second.
- **Sixel decoding.** Now decoded straight into one flat buffer of final
  pixels (~193 MB/s headless at 26.11.51); remaining candidates are SIMD
  band decoding and streaming decode instead of buffering the whole DCS.
- **Kitty image decoding.** Raw RGBA parses at ~1.2 GB/s and zlib through
  our own inflate at ~304 MB/s (26.11.51); PNG, at ~144 MB/s, spent ~60%
  inflating and ~20% unfiltering inside the PNG crate when last profiled. Our own PNG decoder (our inflate plus a
  SIMD unfilter) is the next step; the RGBA path could decode base64
  straight into pixels instead of through a byte buffer.
- **Rendering during floods:** frames still write unchanged background pixels,
  competing with parsing for memory bandwidth. Evaluate skipping those pixels
  or lowering internal resolution, retaining a change only with repeatable
  throughput and visual gains.
- **Parsing behind a PTY:** the parser thread does the same work 20–35%
  slower than headless (`scroll_region_up`, `sgr`, `attach_to_previous`
  in `perf`), so `+pipeline raw` trails `parse` by about 20%. The reader
  thread is mostly idle; find what the parser competes for (caches, memory
  bandwidth, the reader's copies) and close the gap.
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

Open:

- A staged PGO + BOLT pipeline producing the binary in every release
  artifact ([#34](https://github.com/poopware-laboratories/poopterm/issues/34));
  PGO (#32) and BOLT (#33) work for local builds already.
- Sample-based PGO (AutoFDO) from perf profiles of real use, with nightly
  `-Zprofile-sample-use` ([#35](https://github.com/poopware-laboratories/poopterm/issues/35)).
- Hot-function ordering and identical code folding at link time, which
  also works on macOS ([#38](https://github.com/poopware-laboratories/poopterm/issues/38)).

Done (the issues hold the numbers):

- Runtime CPU dispatch to x86-64-v3 copies of the tokenizer,
  printing, cell fills, UTF-8 validation and glyph blending (#36, #125);
  scrollback as one contiguous ring instead of a `Vec` per row (#40).
- **The 26.10.23–26.10.27 regression is recovered.** That compatibility
  work cost 2–4% on tui, editor, cursor and progress and about 6% on
  unicode. Measured again at 26.10.83 (i3-N305, release, `--benchmark`
  parse MB/s, pinned to one core, 4 alternating runs each, medians):
  log 208→277, unicode 89→152, tui 180→219, editor 161→261, progress
  189→365, cursor 118→182 against 26.10.22. What it added on the hot path
  (one LNM branch per line feed, a few CSI arms, new `Term` fields) is
  layout cost that #39 covers.

## Hardware-accelerated rendering

Full GPU rendering, now that kitty graphics has landed. A full software
redraw takes about 1 ms at 1000×600 and 2 ms at 1920×1080 (scale 2) on an
Intel N-series; it also takes memory bandwidth from the parser during
floods, which is why floods are capped at 30 fps. Parse throughput stays
CPU work. Baseline before the chrome moves over, from
`POOPTERM_FRAME_STATS`: [performance.md](docs/performance.md#gpu-baseline-2026-11-01).
Today the GPU path is slower than software (1.2 vs 1.0 ms a frame, 1.3 vs
0.4 ms from key to frame at 1000×600), because of the full backdrop it
uploads each frame.

Tracked in [#30](https://github.com/poopware-laboratories/poopterm/issues/30), with
one issue per piece (#21–#29; the renderer boundary, #20, is done). All of it is claimed by @vx9k; ask on the
issue before starting a piece.

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
- **Status:** `gpu::vk::Renderer` draws a `QuadPen` frame offscreen, and
  every shared grid scene matches software within one step a channel per
  translucent layer (software truncates each blend, the GPU rounds it), on
  Intel and llvmpipe. `gpu::vk::window::GpuWindow` picks the device for a
  window's surface and shows frames on a swapchain kept the window's size
  (passing the Khronos validation layer on a headless surface). The main
  window draws whole frames through it on Linux and FreeBSD (grid, chrome,
  wallpaper and images, all as quads), and so does the Settings window,
  each falling back to software for good if its GPU fails. `POOPTERM_RENDERER` overrides the configured renderer.
  Sixel, kitty and iTerm2 images and the wallpaper are drawn as textures,
  uploaded once and kept while their bitmap lives (256 MiB at most); one
  larger than the device makes or the budget holds is shrunk on the CPU
  to the size it is shown at (`gpu::shrink`). A frame lacking room in the
  glyph atlas grows it (up to 256 MiB) and is drawn again. A frame the
  GPU still refuses (a glyph wider than the device makes, or images that
  together fill the budget a texel each) hands the window to software for
  good, as a GPU failure does; software is otherwise only an opt-in.
  Fractional scales on Wayland draw at full resolution and match software,
  scale changes included. Done on Linux and FreeBSD. Metal (macOS, #26):
  our own Objective-C runtime and Metal bindings (`objc.rs`, `gpu/mtl`),
  the grid renderer (matching software in the pixel-equivalence tests on
  the real GPU) and windows through a `CAMetalLayer` (drawables at the
  window's size in pixels, Retina scale, three in flight, software when
  Metal fails). The default (`renderer = "software"` turns it off). Next:
  frame latency and throughput against software on several Macs, and the
  quads written for frame N while the GPU draws N − 1 (each frame now waits
  for the last before writing).

## Resource management and memory

- **Memory footprint.** Reported at about 400 MB on macOS, staying near
  140 MB afterwards. On Linux (one tab, 20,000 lines of scrollback, 900×600)
  it went from 58 MB idle and 130–230 MB after a flood (26.10.55) to
  20.7 MiB idle, 43 MiB after a flood filling the history and 23 MiB after
  `ESC [3J` in software, and 61, 83 and 63 MiB on the GPU, whose driver
  maps most of the difference (`docs/performance.md`). Still to check: the
  macOS figure. Where the rest goes, worst first:
  - *Fonts are mapped, not copied (done).* The main font, configured
    fallbacks and installed fallbacks are mapped read-only (`mapped.rs`),
    so they cost shared, evictable page cache instead of private memory,
    and only their pages in use are resident. With the CJK, emoji and
    symbol fallbacks loaded, private memory went from 66.5 to 30.8 MiB and
    RSS from 105 to 70 MiB (sway, one tab). Still to check: the macOS
    figure, where system fonts (PingFang, Hiragino, Apple Color Emoji)
    are largest.
  - *The scrollback ring grows by doubling and copying* (`Scrollback::grow`):
    during each step the old and new rings are both live. Grow in
    fixed-size segments without copying (fits #40). `ESC [3J` and a lower
    line limit already give the ring's memory back. It costs about 16 bytes per cell of history
    per tab.
  - *Freed memory after a clear (done).* mimalloc purges at once and
    transparent huge pages are off (`allocator.rs`): after `ESC [3J` the
    terminal is back within 2 MiB of idle, cycle after cycle.
  - *Idle baseline.* 15 MiB private before any output in software: audit
    it (fonts, font database, glyph caches, per-tab buffers sized for the
    largest window).
  - General Rust techniques to apply where profiles point: `shrink_to_fit`
    or dropping buffers after spikes (PTY queue, OSC/DCS payloads, kitty
    transfers), `Box<[T]>` over `Vec<T>` for fixed data, smaller
    enums and indices (`u32`/`u16` over `usize`), interning repeated
    strings (hyperlinks, titles), and lazy initialization of rarely used
    state (`Option<Box<…>>`, `OnceCell`).
  - Track it: `--benchmark --terminal` reports start, end and peak RSS,
    and tests check that clearing history releases its memory. Still to
    check on macOS (`footprint`, `vmmap`).
- **cgroups v2 per tab (Linux): join before `exec`.** The terminal asks
  systemd for a scope with `Delegate=yes` and lays it out itself
  (`src/cgroup_tree.rs`): the terminal in a `terminal` leaf with
  `cpu.weight` 400, each shell in a `tab-<pid>` leaf with `memory.high` at
  7/8 of RAM, the delegated `cpu`, `memory` and `pids` controllers enabled,
  and empty leaves of closed tabs removed. Without systemd, our own cgroup
  is laid out the same way when it is writable and ours alone; a shell no
  leaf can take gets a systemd scope of its own. Still to do: with our own
  PTY layer, the shell joins its leaf before `exec`. Moving the shell
  happens after it starts, so anything it forks in its first milliseconds
  stays behind; `clone3(CLONE_INTO_CGROUP)` can close that gap now that we
  spawn shells ourselves (`src/pty/spawn.rs`).

## Replacing dependencies with our own code

Chosen to replace with our own implementations, built around this
project's needs and optimizations. Each replacement is tested against the
crate it replaces as an oracle (differential tests, and fuzzing for parsers
of untrusted input), and lands only once it matches it and measures at
least as fast.

- **`portable-pty` → our own PTY layer, complete on Linux, macOS and
  FreeBSD.** Clean room: written from POSIX (`posix_openpt`, `grantpt`,
  `unlockpt`, `ptsname`, `setsid`, `tcsetpgrp`, termios) and each system's
  manual pages (Linux `pty(7)`, `ioctl_tty(2)`; macOS and FreeBSD `pty(4)`,
  `tty(4)`), never from portable-pty's or any terminal's source. One module
  per platform where they differ; everything else shared. **Done (#188):**
  portable-pty was the oracle until the replacement matched it, and is no
  longer a dependency at all.
  - **Done: pairs and sizes** (`src/pty/device.rs`). Allocation, both sides
    closed on exec and `O_NOCTTY`, and the window size in cells and pixels,
    tested against portable-pty until it matched, and now against fixed
    expectations.
  - **Done: spawning, per POSIX** (`src/pty/spawn.rs`). In the child, a new
    session (`setsid`), the slave as controlling terminal (`TIOCSCTTY`) and
    stdio, every other descriptor close-on-exec (`close_range` where the
    kernel has it), signal mask and dispositions back to default, `chdir`,
    then `execve` with `argv[0]` kept (`-zsh` for login shells) and `PATH`
    resolved before forking. `exec` and setup failures come back as `errno`
    through a close-on-exec pipe. Only async-signal-safe calls between `fork`
    and `exec`; everything is allocated before forking.
  - **Join the tab's cgroup before `exec` (Linux).** With the delegated
    subtree from the cgroups item, the child writes itself into its tab's
    `cgroup.procs`, or is created there with `clone3(CLONE_INTO_CGROUP)`, so
    nothing the shell starts escapes its group: the gap of today's
    after-the-fact move goes to zero.
  - **Done: termios defaults** (`src/pty/termios.rs`). `IUTF8` where the
    libc crate has it (Linux, macOS; not FreeBSD yet), DEL as erase, the
    usual control characters, line editing, `\n` out as `\r\n` and 38400
    baud, applied to every pair `device::open` makes.
  - **Window size in cells and pixels.** `TIOCSWINSZ` with real
    `ws_xpixel`/`ws_ypixel` (today they are 0), so Sixel, kitty graphics and
    image viewers size images correctly; the kernel then sends `SIGWINCH` to
    the foreground process group.
  - **Foreground process.** `tcgetpgrp` on the master for the foreground
    job: tab titles, working-directory discovery and "a program is running,
    close anyway?" prompts look at it instead of the shell.
  - **Flow control.** Packet mode (`TIOCPKT`) to notice `^S`/`^Q` (`IXON`)
    stops and starts, so the UI can show that output is paused instead of
    looking frozen.
  - **Done: reaping from one thread** (`src/pty/exits.rs`, `reap.rs`). A
    pidfd per shell in an epoll set on Linux, `EVFILT_PROC`/`NOTE_EXIT` in a
    kqueue on macOS and FreeBSD; one `Reaper` thread waits for each child
    as it ends and hands its exit code or signal to the code watching it.
  - **Done: tabs on our own PTY.** Every tab's shell runs on `device`,
    `spawn` and the reaper, read and written through a `File` of the master;
    closing a tab hangs its shell up and reaps it. The benchmark's PTY
    stages followed (#270).
  - **Exit status in the app.** With the reaper: exit codes and signals for
    `OSC 133` command status and the "process exited" line.
  - **Tests.** Side by side with portable-pty while it was here, now
    against fixed expectations: the environment, `argv[0]`,
    controlling terminal and session, window size seen by the child
    (`stty size`, `TIOCGWINSZ`), termios flags, exit codes and signals, and
    byte-identical output for the benchmark corpus. Failure paths: missing
    shell, `exec` failure, closed master, child killed mid-write. Must pass
    CI on Linux, macOS and FreeBSD.
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
  feeding the GPU glyph atlas. Color emoji: CBDT bitmaps and VS16/VS15
  presentation (drawing only; widening on VS16 waits for mode 2027) are
  done, and so are sequences inside one cell (ZWJ, keycaps, tag flags)
  through our own GSUB engine. So are flags from regional indicator pairs
  and skin-tone modifiers: the term keeps them in separate cells, as
  wcwidth counts them, and the renderer draws the font's joined glyph
  across both (`render::cell_joins`). Installed fallback fonts are chosen
  by rank, never by what loaded first (#127). sbix strikes are drawn
  ([#129](https://github.com/poopware-laboratories/poopterm/issues/129)): PNG and
  `dupe` records, the largest strike holding a PNG; Apple's compressed
  `emjc` type is skipped. Next: COLR v0 with CPAL, then COLR v1.
- **`softbuffer` → our own present path** (X11 MIT-SHM, Wayland `wl_shm`,
  macOS CALayer) for the software renderer, which becomes the fallback
  beside Vulkan/Metal. Do it together with the GPU work so presentation and
  damage handling are designed once. Done: `present::{pool, copy}` (buffer
  ages, rows to copy and damage), `present::wayland` (libwayland-client
  opened at run time, `wl_shm` on our own event queue), which shows both
  windows' software frames on Wayland, and `present::x11` (libxcb opened at
  run time, MIT-SHM by descriptor, else `PutImage`). Next: show X11 windows
  through it, then macOS CALayer, and drop the crate. Dropping it also
  unblocks Miri: softbuffer's `tiny-xlib` registers a constructor that
  `dlopen`s libX11 at startup, which Miri cannot run, so no test runs under
  it yet. Then run the pure modules (`term`, `parser`, `vt`, `inflate`,
  `graphics`) under `cargo miri test` in CI.

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

Centering the chrome and dropping the "poopterm" label are done: measured
on a 26.11 screenshot, every tab label, the + and settings icons and the
status bar text sit on the bar's middle row, and the first tab starts at
x = 8.

- **One title line on Linux.** Merge the title bar into the tab bar: tabs,
  new tab, settings, then minimize, maximize and close, in one row, with
  the empty part of the row dragging the window (`drag_window`), a double
  click maximizing, and the edges resizing it (`drag_resize_window`). This
  means drawing our own decorations (`with_decorations(false)`) where the
  compositor offers client-side ones (GNOME, KDE on request); keep server
  side decorations where the user or compositor asks for them, and draw no
  buttons on tiling compositors such as Sway. macOS already puts the tabs
  in the title bar beside the system's traffic lights.
  - [x] Model (`ui::window_controls`): the desktop's button layout (GNOME
    `button-layout`, all three on the right otherwise), button places
    (36 × 32, 8 from the edge, a divider 8 either side), hits, and resize
    edges (6 pixels, 12 at corners, none when maximized).
  - [x] Draw the buttons in the header, with hover, pressed, restore and
    inactive states; shift the tabs and the + and settings buttons by the
    reserves.
  - [x] Route presses: buttons, drag on the empty bar, double click to
    maximize, resize edges with their cursors.
  - [x] Choose decorations (`decorations` key): our own where the compositor
    offers no xdg-decoration (GNOME) and the horizontal header is shown;
    read `button-layout` from the settings portal.
  - [ ] Follow `SettingChanged` from the portal, so a new `button-layout`
    applies without a restart.
  - [ ] Client decorations with vertical or hidden tabs: a slim title row
    to hold the buttons, so `auto` can use them there too.

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
- **Reduced motion.** `reduce_motion` holds kitty animations; still to do:
  a Settings toggle, following the desktop's preference where the platform
  exposes one (the settings portal's `org.gnome.desktop.interface`
  `enable-animations`, `NSWorkspace` on macOS), and holding off screen
  flashes (DECSCNM toggled back within a moment, the visual bell).
