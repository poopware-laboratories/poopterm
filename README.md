# poopterm 💩

Poopterm is a lightweight, high-throughput terminal emulator written in Rust: it handles real-world text output at 63–248 MB/s end to end (108 MB/s overall, images included) on a low-power laptop CPU (see [performance](docs/performance.md)). It opens your default shell in a native window, with tabs, clipboard support, and thirteen built-in color themes. Terminal compatibility is growing and is not yet a complete xterm implementation.

![Poopterm app icon](assets/icon.png)

## Features

- Starts the shell in `$SHELL`, falling back to `/bin/sh`.
- Supports basic cursor movement, 16-color, 256-color, and truecolor SGR,
  alternate-screen switching, and common editing and function keys.
- Configurable scrollback (5,000 lines by default). Use the mouse wheel to browse it.
- Supports drag-to-select text (hold Alt to select a rectangle) and system clipboard copy/paste.
- Keeps combining marks attached to their base cell for display and copy, and can load a user font with the bundled font as a missing-glyph fallback.
- Opens independent shell sessions in tabs, with activity and bell indicators.
- Searches scrollback with a highlighted match and next/previous navigation.
- Restores up to 12 tabs and their directories when reopening the window.
- Opens HTTP(S) links, OSC 8 hyperlinks, and local `file://` links with a modifier-click; modifier-clicking a displayed path copies it.
- Uses a compact tab bar and a status bar showing directory, grid size, and theme.
- Supports bracketed paste and common SGR mouse reporting modes.
- Supports the initial kitty keyboard protocol (`CSI u`) key encoding for applications that opt in.
- Resizes the shell along with the window.
- Batches synchronized terminal updates (`?2026`) to avoid partial frames.
- Displays Sixel images and the kitty graphics protocol (including animation
  and Unicode placeholders) within explicit memory limits, and tracks OSC 133
  shell markers.
- Supports semicolon/colon truecolor, DEC line drawing, and additional text styles.
- Includes Dark Mode, Light Mode, Nord, all three Rosé Pine variants, all four Catppuccin flavors, Bark, AMOLED Black, signature Poop, and a configurable Custom theme.
- Includes an **About poopterm** page in Settings.
- Supports PNG, JPEG, and WebP terminal wallpapers with adjustable strength in Settings.
- Lets macOS users adjust main-window opacity from Settings.
- Includes a desktop launcher and icon for Linux and FreeBSD.
- On Linux, runs each tab's shell in its own cgroup (a delegated systemd scope, or the terminal's own cgroup without systemd), with the terminal weighted above the tabs and each tab's memory capped below the machine's, so a runaway program in one tab cannot starve the terminal or the other tabs.
- **Fast output:** a background PTY reader with backpressure, redraws paced to the display, an O(columns) row-based grid with 16-byte cells, a portable-SIMD fast path for plain text, and synchronized updates that cost one screen copy however long the scrollback is. Run `scripts/bench-throughput.py` in any terminal to compare.

## Build and run

Poopterm currently requires **nightly Rust** because its optimized pixel blending uses Rust's `portable_simd` feature. The included `rust-toolchain.toml` selects nightly automatically when Rust is installed through rustup.

Build from the project directory:

```sh
cargo run --release
```

To build without launching the app, run `cargo build --release`.

Released packages are profile-guided optimized builds. To build one
yourself, `make pgo` trains on the built-in benchmark (about 5% faster than
a plain release build, 13% on interactive output); `make pgo-use`
trains it further on your own sessions, `make bolt` lays it out with BOLT on
Linux (about 3% more) and `make pgo-install` installs what releases ship.
See [`docs/performance.md`](docs/performance.md#profile-guided-optimization).

## Package managers

The Homebrew formula is available from the Poopterm tap. Install it with:

```sh
brew tap poopware-laboratories/poopterm https://github.com/poopware-laboratories/poopterm.git
brew install poopware-laboratories/poopterm/poopterm
```

### Debian

Poopterm packages for 64-bit Debian are published through the [pooplabs APT
repository](https://github.com/poopware-laboratories/pooplabs-apt-repo). Add the
repository, refresh APT, and install the package:

```sh
echo 'deb [trusted=yes] https://poopware-laboratories.github.io/pooplabs-apt-repo/ ./' \
  | sudo tee /etc/apt/sources.list.d/pooplabs.list
sudo apt update
sudo apt install poopterm
```

The repository is unsigned, so APT's `trusted=yes` option skips package
signature verification. Install from it only if you trust Poopterm Labs. The
package includes the desktop launcher, application icons, and terminfo entry;
APT installs required system libraries automatically. The current package is
built for `amd64` and requires Debian 12 or newer.

### Fedora

Each [release](https://github.com/poopware-laboratories/poopterm/releases) also
carries an RPM package for 64-bit Fedora and related distributions. Download
it and install it with DNF, which pulls in the display libraries it needs:

```sh
sudo dnf install ./poopterm-26.10.93-1.x86_64.rpm
```

It ships the same profile-guided build, desktop launcher, icons and terminfo
entry as the Debian package. There is no DNF repository yet, so updates are
installed the same way.

### Arch Linux

The repository carries a PKGBUILD that builds the latest release from source
with the nightly toolchain, which it installs inside the build directory:

```sh
curl -LO https://raw.githubusercontent.com/poopware-laboratories/poopterm/main/packaging/arch/PKGBUILD
makepkg -si
```

Each package attached to a [release](https://github.com/poopware-laboratories/poopterm/releases)
carries a signed build provenance attestation, which proves that the release
workflow built it from the tagged commit. Check a downloaded package with the
GitHub CLI:

```sh
gh attestation verify poopterm_26.10.93-1_amd64.deb --repo poopware-laboratories/poopterm
```

On macOS, Homebrew builds both the `poopterm` command and a `Poopterm.app`
bundle containing the `.icns` app icon. The bundle lives in Homebrew's managed
prefix. Open it directly with:

```sh
open "$(brew --prefix)/opt/poopterm/Poopterm.app"
```

To make the app appear in your user Applications folder, create a symlink:

```sh
mkdir -p ~/Applications
ln -sfn "$(brew --prefix)/opt/poopterm/Poopterm.app" ~/Applications/Poopterm.app
```

The link follows Homebrew upgrades. The `poopterm` command also remains
available for launching the app from a terminal.

### Build dependencies

On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libx11-dev libxkbcommon-dev libwayland-dev
```

On Fedora:

```sh
sudo dnf install gcc pkgconf-pkg-config libX11-devel libxkbcommon-devel wayland-devel
```

On Arch Linux:

```sh
sudo pacman -S base-devel libx11 libxkbcommon wayland
```

On macOS, install the Xcode Command Line Tools if they are not already present:

```sh
xcode-select --install
```

On FreeBSD, install Rust, pkgconf, and the display-system libraries you use:

```sh
pkg install rust pkgconf xorg libX11 libxkbcommon wayland
```

## Themes

Poopterm includes Dark Mode, Light Mode, the official Nord palette, all three
Rosé Pine variants (Main, Moon, Dawn), all four Catppuccin flavors, Bark, AMOLED
Black, and signature Poop themes. Choose a theme in **Settings** or press
**Ctrl+Shift+T** to cycle through them. The selection is saved to a config file
and restored on the next launch. AMOLED Black uses a pure black background with
warm gold accents.

The default config path is `~/Library/Application Support/poopterm/config.toml`
on macOS and `~/.config/poopterm/config.toml` on Linux and FreeBSD (or
`$XDG_CONFIG_HOME/poopterm/config.toml` when `XDG_CONFIG_HOME` is set). Create
the file or open **Settings** from the gear button in the tab bar. Settings
opens in a separate window with a theme picker, wallpaper chooser and strength
control, font-size and scrollback controls, session restore, tab visibility and
orientation, compact tabs, tab-button visibility, and a bottom-bar toggle.
Compact mode gives the active horizontal tab more space while shrinking inactive
tabs. In vertical mode it collapses the sidebar into a narrow icon rail. Click
**Save** to apply the preferences:

```toml
theme = "catppuccin-mocha"
font_size = 18          # Points, 10–32; fractions such as 13.5 work
scrollback_lines = 5000 # 0–100000; 0 disables history
restore_sessions = true
opacity = 100           # macOS main window opacity, 60–100
show_tabs = true
vertical_tabs = false
compact_tabs = false
show_new_tab_button = true
show_settings_button = true
show_bottom_bar = true
padding = 0             # Space around the grid in logical pixels, 0–32
reduce_motion = false   # Hold kitty graphics animations on their current frame
wallpaper = "/Users/you/Pictures/terminal.webp" # optional PNG, JPEG, or WebP
wallpaper_strength = 35 # 0–100; 0 hides the image, 100 shows it at full strength
shortcut_new_tab = "super+t"
shortcut_command_palette = "ctrl+shift+p"
shortcut_cycle_theme = "ctrl+shift+t"
shortcut_search = "super+f"
shortcut_close_tab = "super+w"
```

`renderer` accepts `auto` (the default), `gpu` and `software`. There is no GPU
backend yet ([#30](https://github.com/poopware-laboratories/poopterm/issues/30)),
so all three draw in software today; `auto` and `gpu` will fall back to software
when GPU startup fails.

`theme` accepts `darkmode`, `light-mode`, `nord`, `rose-pine` (or the legacy
`rosepine` alias), `rose-pine-moon`, `rose-pine-dawn`, `catppuccin-latte`,
`catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, `bark`,
`amoled-black`, `poop`, or `custom`. The custom theme starts from the Poop
palette and accepts optional six-digit RGB overrides. Color keys can also
override a built-in theme; AMOLED Black keeps its pure black background:

```toml
theme = "custom"
foreground = "#E8D5B0"
background = "#24170B"
border = "#7A4E1F"
color0 = "#1E1409"
color1 = "#D9534F"
color2 = "#8BC34A"
color3 = "#E0B040"
color4 = "#5C9BD6"
color5 = "#B06AC8"
color6 = "#4FB6B0"
color7 = "#E8D5B0"
font_family = "JetBrains Mono"
```

`font_family` picks an installed font by family name (any case). Poopterm keeps
a database of installed fonts in `~/.cache/poopterm/fonts.db` (under
`$XDG_CACHE_HOME`), checks it against the font directories' modification times
at startup, and reads only new or changed fonts when they differ.
`font_path` or the `POOPTERM_FONT` environment variable selects a font file
instead, and takes precedence over `font_family`; `POOPTERM_FONT` takes
precedence over `font_path`. Glyphs missing from a custom font fall back to the
bundled DejaVu Sans Mono for Powerline font. Emoji draw in color from an installed color bitmap font (CBDT, such as Noto Color Emoji), including ZWJ sequences, keycaps, flags and skin tones; sbix (Apple Color Emoji) and COLR fonts are not read yet. For complete htop or other Nerd
Font icon coverage, choose a Nerd Font Mono family or file. Use a monospaced
font for predictable cell alignment.
The older `macos-dark` and `macos-light` values remain accepted for existing configs.
The grid fills the window by default; `padding` (also on the Settings Layout
page) adds space around it. Whatever the padding, the space left over where the
window is not a whole number of cells takes the colors of the cells at the
grid's edges, so full-screen programs' backgrounds reach the window edge.
Settings changes apply immediately after saving; session restore takes effect on
the next launch. Reducing scrollback discards the oldest lines. Invalid numbers
fall back to defaults or are clamped to supported limits. Other config keys and
comments are preserved when Settings saves. Toolbar text keeps its own size.

On macOS, use the **Window opacity** controls in Settings and save to make the
main window translucent. This applies to the entire terminal window; the separate
Settings window stays opaque. The default is 100%. This setting currently works
on macOS only; blur is not implemented.

Set `POOPTERM_CONFIG` to use a config file at another path. `POOPTERM_THEME`
can override the saved setting for a single launch, for example:

```sh
POOPTERM_THEME=catppuccin-mocha cargo run --release
```

```sh
POOPTERM_THEME=catppuccin-latte poopterm
```

## Tabs and keyboard shortcuts

- **Cmd+T** on macOS or **Ctrl+Shift+N** elsewhere: open a tab.
- **Ctrl+Tab** / **Ctrl+Shift+Tab**: move to the next or previous tab.
- **Cmd+W** on macOS or **Ctrl+Shift+W** elsewhere: close the active tab.
- **Cmd+1–9** / **Ctrl+1–9**: jump directly to a tab.
- **Cmd+F** / **Ctrl+Shift+F**: search the current tab's scrollback. Type a query,
  press **Enter** / **Shift+Enter** for next/previous, and **Esc** to close.
  Search is case-insensitive for ASCII and matches within each displayed row
  (up to 10,000 matches per query).
  The usual paste shortcut can paste a search query.
- **Cmd+Up/Down** on macOS or **Ctrl+Shift+Up/Down** elsewhere: jump to the
  previous or next shell prompt (needs shell integration).
- **Cmd+= / Cmd+- / Cmd+0** on macOS, or **Ctrl+= / Ctrl+- / Ctrl+0**
  elsewhere: make the terminal font larger or smaller (6 to 72 points), or put
  it back to the size in Settings. Zoom lasts for the session; Settings keeps
  the saved size.
- **Cmd-click** / **Ctrl-click** an HTTP(S) URL to open it in your browser, or
  a displayed path containing `/` to copy it. Links to executables and
  launcher files (`.desktop`, `.command`, …) are copied, never opened; the
  status bar shows a hyperlink's real target while you hover it. Mouse-reporting terminal programs
  retain their own click handling.
- Click a tab to switch sessions, or click **+** to open one.
- On macOS, tabs share the transparent titlebar beside the traffic lights. On
  Linux, they remain in the separate tab row.
- Click the gear button in the tab bar to open Settings. Click a tab's
  **×** to close it.
- **Cmd+C / Cmd+V** on macOS, or **Ctrl+Shift+C / Ctrl+Shift+V** elsewhere:
  copy a selection or paste clipboard text.
- Drag with the left mouse button to select text. The mouse wheel scrolls
  scrollback unless the running app has enabled mouse reporting.
- **Ctrl+Shift+T** cycles themes and saves the choice to the config file.
- **Ctrl+Shift+P** opens the command palette for new tab, settings, About, theme,
  copy-last-command-output, close-tab, and rename-tab actions. Enter filters and runs the first match; after
  choosing rename, type a title and press Enter.
- Open Settings with the gear button or from the command palette. Choose
  **Shortcuts**, click a binding, and press a new key chord; press **Esc** to
  cancel recording. Click **Save** to apply your changes. The shortcut page also
  lists each action's current binding.

The `shortcut_new_tab`, `shortcut_command_palette`, `shortcut_cycle_theme`,
`shortcut_search`, `shortcut_close_tab`, `shortcut_previous_prompt`,
`shortcut_next_prompt`, `shortcut_zoom_in`, `shortcut_zoom_out`, and
`shortcut_zoom_reset` config keys accept `ctrl`, `shift`, `alt`, and `super`
plus a character key or one of `up`, `down`, `left`, `right`, `pageup`,
`pagedown`, `home`, `end`, `tab`, `enter`, `space`, or `plus`. Defaults use the platform's common
shortcuts shown above: **Cmd+T** / **Ctrl+Shift+N** opens a tab, **Ctrl+Shift+P**
opens the command palette, **Ctrl+Shift+T** cycles themes, **Cmd+F** /
**Ctrl+Shift+F** searches, **Cmd+W** / **Ctrl+Shift+W** closes a tab, and
**Cmd+Up/Down** / **Ctrl+Shift+Up/Down** jumps between prompts, and **Cmd** /
**Ctrl** with **=**, **-** or **0** zooms. The sample values
above are for macOS. Tab cycling, direct tab selection, and clipboard shortcuts
are fixed bindings.

On Linux, clipboard shortcuts need a clipboard command-line tool: `wl-clipboard`
for Wayland (`wl-copy` and `wl-paste`), or `xclip`/`xsel` for X11.

### Shell integration

Poopterm's hooks report command start/exit and the current working directory.
They power prompt jumps, "Copy last command output", and a tab marker (plus a window attention request) when a
command of 10 s or more finishes while you are not looking.

**zsh, fish, and bash load them automatically**, without editing your startup files
(zsh through a temporary `ZDOTDIR` that then restores yours, fish through
`XDG_DATA_DIRS`, bash through `--rcfile`). Bash's loader reads `/etc/profile`
and the first available login profile before installing hooks, or `.bashrc`
when no login profile exists. Bash ignores `--rcfile` with `-l`, so this shell
is interactive but does not set bash's `login_shell` flag. Set `shell_integration = "off"` in `config.toml` to disable
this. The hooks also make `ssh` connect with `TERM=xterm-256color`, since
remote hosts rarely have Poopterm's terminfo entry; set
`POOPTERM_SSH_KEEP_TERM=1` to keep it. fish 4 marks its prompts itself
(its `mark-prompt` feature), so there the fish hook only reports the
directory.

To install the hooks by hand, review the output and append it to
the startup file for the shell you use:

```sh
poopterm --print-shell-integration zsh >> ~/.zshrc
poopterm --print-shell-integration bash >> ~/.bashrc
poopterm --print-shell-integration fish >> ~/.config/fish/config.fish
```

Use only the command matching your shell. Restart the shell after adding it.
Remove the generated block between the `poopterm shell integration` comments to
disable the hooks. Without them, Poopterm still launches the
shell and tracks OSC 7/133 markers emitted by other integrations.

### Working directories and session restore

New tabs inherit the active shell's directory. macOS and Linux discover the
shell directory when opening a tab or closing the window. Shells can also report
it using OSC 7. On BSD, install the optional shell hooks above to report
directory changes. A directory reported from another host, such as a shell over
ssh, is not used for new tabs; the status bar names that host instead.
Close the window to save its tabs; reopening starts fresh login shells in the
saved directories. Running programs and terminal output are not restored.
Closing a tab removes it from the saved list, including the last tab.
State is stored in `sessions.txt` in the state directory (`$XDG_STATE_HOME/poopterm`,
or `~/Library/Application Support/poopterm` on macOS), with a limit of 12
restored tabs. Missing directories fall back to the home directory.

A dot marks unseen background output; `!` marks a terminal bell. Switching to a
tab clears its indicator.

## Install on Linux or FreeBSD

Install system-wide, so every user, desktop session, and `sudo` finds
Poopterm: the binary goes to `/usr/local/bin`, the desktop entry and icons to
`/usr/local/share`, so GNOME, KDE, and other desktops list it in their app
menus, and the terminfo entry to `/usr/local/share/terminfo`. Run the script
as yourself: it builds with cargo and asks for root (sudo or doas) only to
copy the files.

```sh
scripts/install.sh
scripts/install.sh --uninstall
```

`make && sudo make install` does the same. `PREFIX` and `DESTDIR` are
honored, for packaging. Poopterm keeps its config in `$XDG_CONFIG_HOME/poopterm`, open tabs in `$XDG_STATE_HOME/poopterm`,
and generated files in `$XDG_DATA_HOME/poopterm`.

## Login shell

Poopterm launches the shell from `$SHELL` (falling back to `/bin/sh`) as a login
shell on macOS, Linux, and BSD. For zsh, this loads both `~/.zprofile` and
`~/.zshrc`.

## macOS app bundle

Put Homebrew's `shellenv` setup in `~/.zprofile` so GUI-launched shells inherit
its PATH. For Apple Silicon:

```sh
eval "$(/opt/homebrew/bin/brew shellenv)"
```

For Intel Macs, use `/usr/local/bin/brew` instead. Commands such as `hyfetch`
must be installed and available on the PATH in that login shell.

## Compatibility notes

Poopterm supports alternate-screen programs such as `nano` and `btop`, common
cursor and editing sequences, device status replies, scroll regions, 256-color
and truecolor output, wide-character cell spacing, combining marks, text selection,
clipboard shortcuts, bracketed paste, and common mouse reporting. It remains a
small emulator rather than a complete xterm implementation; advanced terminal
applications may rely on sequences or input modes that are not supported yet.
Nerd Font icon glyphs are not bundled, but you can select a Nerd Font Mono file
with `font_path` or `POOPTERM_FONT`.

## Font and license

Poopterm bundles DejaVu Sans Mono for Powerline as `assets/font.ttf`. The font is
distributed under the Bitstream Vera license, with public-domain changes. See
the [DejaVu font license](https://dejavu-fonts.github.io/License.html). A custom
font falls back to the bundled font for missing glyphs; Poopterm also searches
common system font directories for up to eight TTF fonts with Nerd Font, Symbol,
or Awesome in their filenames. Characters none of those fonts have (CJK, math,
rare scripts) are drawn from an installed font that has them, found through
the font cache and always the same one: monospace before proportional, upright
regular weights first, then smaller files. For the broadest htop icon coverage, set `font_path` to a Nerd
Font Mono file.

Poopterm is free software: you can redistribute it and/or modify it under the
terms of the [GNU Affero General Public License](LICENSE) as published by the
Free Software Foundation, either version 3 of the License, or (at your option)
any later version. It is distributed in the hope that it will be useful, but
WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE. Earlier releases were published under the
MIT License.

Copyright © 2026 poopware-labs and contributors. Third-party palette notices
(Catppuccin, Nord, and Rosé Pine) are in
[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES); the bundled font keeps its own
license, above.

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
  automatically (see [Shell integration](#shell-integration)). The hooks also emit
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

## Terminfo

Shells run with `TERM=xterm-poopterm`, an entry that advertises only what
Poopterm supports (truecolor, styled underlines, synchronized output, focus
events, …), when it is installed system-wide; otherwise they get
`TERM=xterm-256color`. A per-user copy (`~/.terminfo`) is not used, because
`sudo` cannot see it and programs such as `sudo dmesg` would lose their
colors. `make install` installs the entry system-wide, or run
`poopterm --print-terminfo | sudo tic -x -`. Set `term = "xterm-256color"`
(or any name) in `config.toml` to override.

## Accessibility

[`ACCESSIBILITY.md`](ACCESSIBILITY.md) describes what works today (keyboard
shortcuts for every action, font sizes up to 32 points (72 with zoom), custom high-contrast
colors, no blinking), the known gaps such as screen reader support, and how
to report an accessibility problem.

## Development and releases

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
./target/release/poopterm --version
```

Development is test-driven. Start with [`AGENTS.md`](AGENTS.md) (for humans
and AI coding agents alike), then see [architecture](docs/architecture.md),
[testing](docs/testing.md), [performance](docs/performance.md) and
[unsafe code](docs/unsafe.md).
CI builds and checks macOS, Linux, and a real FreeBSD VM.

Want to help? Read [CONTRIBUTING.md](CONTRIBUTING.md) and our
[Code of Conduct](CODE_OF_CONDUCT.md). Report security problems privately as
described in [SECURITY.md](SECURITY.md), never in a public issue.

Releases are numbered `YY.M.PATCH`. After all checks pass on `main`, CI
increments the patch, or starts a new month at `YY.M.0` with a new random
codename. It updates `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` and the
Homebrew formula in one commit, pushes it with its tag in a single atomic
push, then attaches the Debian and RPM packages to a draft release and publishes it,
so published releases never change. If `main` moved in the meantime, the run
skips and the newer commit's run releases instead.
Published tags never move. These commits are pushed with a deploy key, the
only bypass of the protection on `main`, and carry `[skip ci]`. Changes reach `main` only through pull requests, squashed
after CI passes; see [AGENTS.md](AGENTS.md#branches-pull-requests-and-commits).
