# Protocols and compatibility

What Poopterm understands beyond the basics, and where it stops. Every
sequence follows its specification, including queries and replies; unknown
or unsupported sequences are ignored silently, never echoed. Everything a
program can make the terminal store has an explicit limit.

## Compatibility

Poopterm supports alternate-screen programs such as `nano` and `btop`, common
cursor and editing sequences, device status replies, scroll regions, 256-color
and truecolor output, wide-character cell spacing, combining marks, text selection,
clipboard shortcuts, bracketed paste, and common mouse reporting. It remains a
small emulator rather than a complete xterm implementation; advanced terminal
applications may rely on sequences or input modes that are not supported yet.
Nerd Font icon glyphs are not bundled, but you can select a Nerd Font Mono file
with `font_path` or `POOPTERM_FONT`.

## Modern terminal protocols

- **Synchronized output:** `ESC [ ? 2026 h` opens an update block; `ESC [ ? 2026 l`
  commits it. Grid changes stay hidden until the commit. A 250 ms timeout prevents
  a broken application from leaving the display frozen. `CSI ? 2026 $ p` queries
  the mode. See the [protocol specification](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md).
- **Shell integration:** OSC 133 marks prompts, command input, output and
  completion, following the [semantic prompts proposal](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md):
  `L` and `A`/`N` start a fresh line, `P` and `k=` mark continuation and right
  prompts, `I` works like `B`, `aid=` tracks nested programs so `N` finishes
  the right command, and `D` takes an exit code and `err=`. With `cl=`, a plain
  click in the line being edited moves the cursor with arrow keys; with kitty's
  `click_events=1` (screen rows) or `2` (rows from the prompt line above), clicks
  in the prompt reach the shell as SGR mouse reports instead, as fish 4 asks.
  kitty's `cmdline=` and `cmdline_url=` on `C` name the running command. Poopterm
  retains the latest 256 markers and shows the running command or exit status
  in the status bar. Poopterm's own hooks for zsh, bash and fish load
  automatically (see [Shell integration](configuration.md#shell-integration)). The hooks also emit
  OSC 7 so BSD shells can report their working directory. Bash 4.4 or later
  reports the running command from its history, so commands history skips
  (`ignorespace`, `ignoredups`) show none.
- **VS Code shell integration:** OSC 633 `A` to `D` mark commands like OSC 133,
  `E` reports the command line, and `P;Cwd=` the working directory. Other
  properties and the nonce are ignored.
- **Contexts:** OSC 3008 ([UAPI.15](https://uapi-group.org/specifications/specs/osc_context/))
  contexts are tracked up to 32 deep, with invalid fields dropped and resets
  leaving them in place. While you act as another user (`run0`, `elevate`,
  `chpriv`), on a remote host, or in a container or VM, the status bar names
  it in front of the directory.
- **iTerm2 commands:** OSC 1337 supports:
  - `CursorShape`, `HighlightCursorLine` (a cursor guide), `SetMark` (a
    prompt-jump target), `StealFocus`, `RequestAttention` and
    `ClearScrollback`;
  - `CurrentDir`, and `RemoteHost`, which the status bar names when it is
    not this machine;
  - `SetUserVar` and `ReportVariable` (only `user.` variables are reported);
  - `Copy` and `CopyToClipboard`/`EndCopy`, up to 1 MiB;
  - `SetColors` and `ReportCellSize`;
  - `RequestUpload`, answered with `abort`;
  - inline images (`File=`, or `MultipartFile`/`FilePart`/`FileEnd`):
    PNG, JPEG and WebP, sized in cells, pixels, percent or automatically,
    up to 32 MiB per file; files that are not inline (downloads) are
    ignored;
  - tab colors (OSC 6;1;bg) and OSC 21337 session status: an indicator
    dot and a colored status after the tab title.

  Commands that need features Poopterm lacks (profiles, the Touch Bar,
  annotations, blocks, buttons, custom sequences) or the user's
  confirmation (background image files, `OpenURL`) are ignored.
- **Clipboard:** OSC 52 writes the clipboard (`c`, `s`, `s0`) and the
  primary selection (`p`) up to 1 MiB; kitty's OSC 5522 adds chunked writes
  in any MIME type, up to 64 MiB. Read requests are refused, so programs
  cannot read clipboard contents without consent.
- **Colors:** every xterm and kitty color sequence, with queries: OSC 4,
  5/6, 10–19 and their resets, kitty's OSC 21, and the color stack
  (XTPUSHCOLORS, XTPOPCOLORS, XTREPORTCOLORS).
- **Notifications:** OSC 9, OSC 777 and kitty's OSC 99 (with buttons,
  icons, closing and activation reports) become desktop notifications,
  through D-Bus on Linux and FreeBSD and `osascript` on macOS, rate limited
  per tab.
- **Program status:** OSC 7501 (working, blocked, done, failed) and OSC 9;4
  progress mark tabs and draw a progress bar.
- **Titles and tabs:** OSC 0, 1 and 2 set the icon name and title, which
  `CSI 22 t` / `CSI 23 t` save and restore. ConEmu's OSC 9;3 names a tab,
  9;9 reports the directory and 9;12 marks a prompt. OSC 22 sets the mouse
  pointer shape.
- **Scaled text:** kitty's [text sizing protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/)
  (OSC 66) draws text up to seven times larger, or at a fraction of a
  cell, following its rules for wrapping, overwriting and erasing.
- **Queries:** DECRQSS (`DCS $ q`) reports SGR, margins, cursor style,
  conformance level and page size; XTGETTCAP (`DCS + q`) answers from
  poopterm's terminfo entry; XTWINOPS reports sizes in cells and pixels
  (`CSI 14/16/18/19 t`). Window moves, resizes and title reports are
  ignored on purpose.
- **Sixel:** DCS `q` images support RGB/HLS registers, repeated columns,
  transparency, raster dimensions, and integer pixel aspect ratios. Defensive
  limits cap an image at 4,096 × 4,096 pixels, 16 megapixels, and 4 MiB encoded.
  Each screen retains at most 16 images and evicts older images to keep its
  retained bitmap budget near 4 megapixels; a single accepted image can exceed
  that budget. Oversized or malformed transfers are discarded. Images scroll
  into the scrollback with the text and are dropped when they leave it; text
  written over an image erases the whole image (partial bitmap erasure is not
  implemented). See [DEC's Sixel reference](https://vt100.net/docs/vt3xx-gp/chapter14.html).
- **Kitty graphics:** APC `G` commands transmit RGB, RGBA and PNG data,
  optionally zlib-compressed, directly (chunked with `m=1`), from a file, a
  temporary file or shared memory; place, query and delete images (every
  `d=` selector, with uppercase freeing); and support z-index layering under
  or over text and backgrounds, relative placements, Unicode placeholders
  (`U=1`), animation frames, composition and terminal-driven playback.
  Images scroll into the scrollback and are cleared by `ED 2`, RIS and the
  alternate screen as the protocol specifies. Limits: 256 MiB per
  transmission, 16,384 pixels per side and 64 megapixels per image, 320 MiB
  of decoded images and 4,096 images per terminal (least recently used,
  off-screen images are evicted first), 65,536 frames per image. File
  transmissions accept only absolute paths outside `/proc`, `/sys` and
  `/dev` (except `/dev/shm`). See the [protocol specification](https://sw.kovidgoyal.net/kitty/graphics-protocol/).
