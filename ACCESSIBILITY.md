# Accessibility

Poopterm should be usable by everyone who uses a terminal, including people
who rely on a keyboard alone, need large text or high contrast, are
sensitive to motion, or use assistive technology. This page says what works
today, what does not yet, and how to tell us when something gets in your way.
It is honest rather than aspirational: a gap listed here is a bug we intend
to fix, not a design decision.

## Goals

- **Keyboard first.** Every action is reachable from the keyboard, and the
  terminal's own actions can be rebound.
- **Readable text.** Text in every built-in theme should meet the
  [WCAG 2.2](https://www.w3.org/TR/WCAG22/) AA contrast ratio (4.5:1) for
  normal foreground on background, and color is never the only signal.
- **No motion you did not ask for.** Poopterm itself never blinks or
  animates; motion only comes from programs you run.
- **Assistive technology.** Screen readers should be able to read the
  terminal, through our own implementation of each platform's accessibility
  interface (AT-SPI on Linux and FreeBSD, NSAccessibility on macOS), written
  in-house like the rest of Poopterm.

## What works today

- **Keyboard.** Tabs, search, prompt jumps, copy and paste, themes, the
  command palette and Settings all have shortcuts. Settings → Shortcuts and
  the `shortcut_*` keys in `config.toml` rebind them (see the
  [README](README.md#tabs-and-keyboard-shortcuts)). In Settings, Enter or
  Ctrl+S / Cmd+S saves, Esc goes back or closes, and the arrow keys change
  the theme and the font.
- **Text size.** `font_size` goes from 10 to 32 points, in steps as
  fine as 0.01 (Settings steps by 0.5), and follows the display scale
  factor, so HiDPI and desktop scaling enlarge it further.
  Any installed font can be chosen in Settings → Fonts. Ctrl+= / Ctrl+- /
  Ctrl+0 (Cmd on macOS) zoom the font from 6 to 72 points for the session,
  and can be rebound like the other shortcuts.
- **Contrast and color.** Every built-in theme's default text foreground and
  background meet WCAG AA contrast (4.5:1). AMOLED Black uses pure black,
  and the `custom` theme or color keys override any color, so you can build
  a high-contrast palette (see [Themes](README.md#themes)). Programs can also
  change the palette through OSC 4, 10, 11 and 12.
- **No blinking.** Blinking cursor shapes (DECSCUSR) and blinking text
  (SGR 5) are drawn steady.
- **Reduced motion.** `reduce_motion = true` in `config.toml` holds kitty
  graphics animations on their current frame; programs can still choose a
  frame themselves.
- **Input methods.** Text committed by an input method (IME) is sent to the
  shell.

## Known limitations

| Limitation | Workaround |
|------------|------------|
| Screen readers cannot read the window: Poopterm exposes no accessibility tree yet. | Use a terminal or console your screen reader already supports until then; within Poopterm, output can be copied to another app with the selection and clipboard shortcuts. |
| Input method preedit (the text you compose before committing) is not drawn in the window. | Most input methods show their candidate window; compose there. |
| Some Settings controls (toggles, font size, scrollback, wallpaper) need a mouse. | Every one is also a `config.toml` key; edit the file and restart. |
| `reduce_motion` is not in Settings yet and does not follow the desktop's reduced-motion preference. | Set `reduce_motion = true` in `config.toml`. |

## Supported environments

Poopterm runs on Linux (Wayland and X11), macOS and FreeBSD. The
limitations above apply to all of them; there is no platform with fuller
support today.

## For contributors

Changes to what users see or do follow the accessibility checklist in the
[pull request template](.github/pull_request_template.md):

- New actions work from the keyboard and get a bindable shortcut.
- Meaning is never carried by color alone, and new colors are checked in
  every built-in theme, light ones included.
- Nothing new blinks, flashes or animates on its own.
- New UI scales with the font size and the display scale factor.

Accessibility work follows the same rules as everything else: test first,
the 3:1 ratio, and no new crates. Platform accessibility interfaces are
implemented from their specifications, like our D-Bus and clipboard code.

## Reporting a problem

Open an [accessibility report](https://github.com/poopware-laboratories/poopterm/issues/new?template=6-accessibility.yml).
It asks what you were trying to do, what got in the way, and which
assistive technology you use, if any. You do not need to know the cause or
a fix.

We read every report and aim to reply within a week, with a workaround when
we know one. Reports that block a core task (starting a shell, reading
output, typing) are fixed before other work. If a fix needs a larger piece
of work, such as screen reader support, we link the report to the issue
that tracks it, so you can follow its progress.
