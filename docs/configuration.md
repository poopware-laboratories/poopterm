# Configuration

Everything Poopterm can be told: the config file and its keys, themes and
fonts, environment variables, shortcuts, shell integration, sessions and
terminfo. Settings (the menu button, ☰, by the tabs) edits the same file.

- [Config file](#config-file)
- [Themes](#themes)
- [Fonts](#fonts)
- [Layout and window](#layout-and-window)
- [Environment variables](#environment-variables)
- [Shortcuts](#shortcuts)
- [Shell integration](#shell-integration)
- [Working directories and session restore](#working-directories-and-session-restore)
- [Login shell](#login-shell)
- [macOS app bundle](#macos-app-bundle)
- [Terminfo](#terminfo)

## Themes

Poopterm includes Dark Mode, Light Mode, the official Nord palette, all three
Rosé Pine variants (Main, Moon, Dawn), all four Catppuccin flavors, Bark, AMOLED
Black, and signature Poop themes. Choose a theme in **Settings** or press
**Ctrl+Shift+T** to cycle through them. The selection is saved to a config file
and restored on the next launch. AMOLED Black uses a pure black background with
warm gold accents.

## Config file

The default config path is `~/Library/Application Support/poopterm/config.toml`
on macOS and `~/.config/poopterm/config.toml` on Linux and FreeBSD (or
`$XDG_CONFIG_HOME/poopterm/config.toml` when `XDG_CONFIG_HOME` is set). Open
tabs are kept in `$XDG_STATE_HOME/poopterm` and generated files in
`$XDG_DATA_HOME/poopterm`. Create
the file or open **Settings** from the menu button (☰) by the tabs. Settings
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

### Theme names and custom colors

`theme` accepts `darkmode`, `light-mode`, `nord`, `rose-pine` (or the legacy
`rosepine` alias), `rose-pine-moon`, `rose-pine-dawn`, `catppuccin-latte`,
`catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, `bark`,
`amoled-black`, `poop`, or `custom`. The custom theme starts from the Poop
palette and accepts optional six-digit RGB overrides. Color keys can also
override a built-in theme; AMOLED Black keeps its pure black background. The older `macos-dark` and
`macos-light` values remain accepted for existing configs.

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

## Fonts

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

Tabs, the status bar, search, the command palette and Settings are drawn in a
proportional UI font: `ui_font_family` when it is installed, otherwise the
first installed of Inter, Adwaita Sans, Cantarell, Noto Sans, Roboto, Ubuntu,
DejaVu Sans and Liberation Sans (SF Pro Text, SF Pro, Helvetica Neue,
Helvetica and Arial on macOS), and the bundled font when none is.

Poopterm also searches common system font directories for up to eight TTF
fonts with Nerd Font, Symbol, or Awesome in their filenames. Characters none
of those fonts have (CJK, math, rare scripts) are drawn from an installed font
that has them, found through the font cache and always the same one:
monospace before proportional, upright regular weights first, then smaller
files.

## Layout and window

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

## Environment variables

Set `POOPTERM_CONFIG` to use a config file at another path. `POOPTERM_THEME`
can override the saved setting for a single launch, for example:

```sh
POOPTERM_THEME=catppuccin-mocha cargo run --release
```

```sh
POOPTERM_THEME=catppuccin-latte poopterm
```

## Shortcuts

The `shortcut_new_tab`, `shortcut_command_palette`, `shortcut_cycle_theme`,
`shortcut_search`, `shortcut_close_tab`, `shortcut_previous_prompt`,
`shortcut_next_prompt`, `shortcut_zoom_in`, `shortcut_zoom_out`, and
`shortcut_zoom_reset` config keys accept `ctrl`, `shift`, `alt`, and `super`
plus a character key or one of `up`, `down`, `left`, `right`, `pageup`,
`pagedown`, `home`, `end`, `tab`, `enter`, `space`, or `plus`. Defaults use the platform's common
shortcuts listed in the [README](../README.md#keyboard-shortcuts): **Cmd+T** / **Ctrl+Shift+N** opens a tab, **Ctrl+Shift+P**
opens the command palette, **Ctrl+Shift+T** cycles themes, **Cmd+F** /
**Ctrl+Shift+F** searches, **Cmd+W** / **Ctrl+Shift+W** closes a tab, and
**Cmd+Up/Down** / **Ctrl+Shift+Up/Down** jumps between prompts, and **Cmd** /
**Ctrl** with **=**, **-** or **0** zooms. The sample values
above are for macOS. Tab cycling, direct tab selection, and clipboard shortcuts
are fixed bindings.

On Linux, clipboard shortcuts need a clipboard command-line tool: `wl-clipboard`
for Wayland (`wl-copy` and `wl-paste`), or `xclip`/`xsel` for X11.

## Shell integration

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

## Working directories and session restore

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

## Terminfo

Shells run with `TERM=xterm-poopterm`, an entry that advertises only what
Poopterm supports (truecolor, styled underlines, synchronized output, focus
events, …), when it is installed system-wide; otherwise they get
`TERM=xterm-256color`. A per-user copy (`~/.terminfo`) is not used, because
`sudo` cannot see it and programs such as `sudo dmesg` would lose their
colors. `make install` installs the entry system-wide, or run
`poopterm --print-terminfo | sudo tic -x -`. Set `term = "xterm-256color"`
(or any name) in `config.toml` to override.
