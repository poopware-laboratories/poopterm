# Feature proposal

What Poopterm could add next, ranked by what users gain for the work. The
first group is built on the local `local/maximized` branch for trying out;
nothing here is pushed yet.

## Built on `local/maximized`

| Feature | What it does | Why |
|---|---|---|
| Follow the desktop's appearance | `theme_light` / `theme_dark` in `config.toml`; the window switches with the desktop. | Most terminals do; people with scheduled dark mode switch by hand today. (Open as #375.) |
| Every action in the command palette | The palette lists every shortcut action (splits, focus moves, search, quick select, zoom, copy, paste, tab moves…) with its chord, not only seven commands. | The palette is how people find features; today most are only reachable by a chord they must already know. |
| Save scrollback to a file | Palette command "Save scrollback to file": the whole history and screen as plain text, in `~/Downloads` (or the home directory) with a dated name. | Keeping a long build log or a session's output without selecting thousands of lines. |
| Broadcast input to panes | Palette command "Broadcast input to all panes": what you type goes to every pane of the tab; the status bar says so. | Running one command on several hosts at once (one SSH session per pane), as iTerm2 and tmux `synchronize-panes` do. |

## Proposed for later

| Feature | Notes |
|---|---|
| Animated GIFs | The decoder already composes every frame with its delay (`src/gif`); play them through the kitty animation frames, holding them under `reduce_motion`. |
| Programming ligatures | Our GSUB engine shapes emoji sequences already; shape runs of same-style ASCII with `liga`/`calt`, cache by run, draw across cells. `ligatures = true` opt-in. |
| Mode 2027 (grapheme clusters) | Width by grapheme cluster when a program asks: emoji ZWJ sequences and VS16 take the width they draw at. |
| Per-tab profiles | `[profile.prod]` sections (theme, font, shell, directory); a palette picker; a red theme for production hosts. |
| Scrollback minimap with prompt marks | OSC 133 marks drawn in the scrollbar; click to jump. |
| Notify on output | "Tell me when this pane prints `ERROR`" or goes quiet for N seconds. |
| Quick terminal | A drop-down window on a global hotkey (macOS and X11; Wayland needs the compositor). |
| Keyboard selection mode | vi-like cursor in scrollback to select without the mouse (accessibility). |
| Theme import | Read iTerm2 `.itermcolors`, Alacritty and base16 files into `custom`. |
| Screen reader support | AT-SPI over our D-Bus client, NSAccessibility on macOS (TODO.md). |
| Kitty remote control | `kitten @`-style commands over the TTY to open tabs and panes from scripts. |
