# Changelog

## 26.10.99 "Giga Fly" — 2026-10-25

### Added

- Antialias rounded corners (#137) (render).

## 26.10.98 "Giga Fly" — 2026-10-25

### Added

- Draw a flat header bar in a proportional ui font (#136) (ui).

## 26.10.97 "Giga Fly" — 2026-10-25

### Fixed

- Make Cell::words_mut an unsafe fn (#135) (term).

## 26.10.96 "Giga Fly" — 2026-10-25

### Fixed

- Point packages and the about page at the moved repository (#132) (release).

### Maintenance

- Rework the readme and move the reference into docs (#133).
- Add diagrams and note boxes (#134).

## 26.10.95 "Giga Fly" — 2026-10-25

### Maintenance

- Fix stale and broken docs and make them consistent (#131).

## 26.10.94 "Giga Fly" — 2026-10-25

### Maintenance

- Inventory unsafe code and bring status and numbers up to date (#130).

## 26.10.93 "Giga Fly" — 2026-10-25

### Fixed

- Choose fallback fonts by rank, not by what loaded first (#128) (render).

## 26.10.92 "Giga Fly" — 2026-10-25

### Added

- Draw flags and skin tones that span two cells as one emoji (#126) (render).

## 26.10.91 "Giga Fly" — 2026-10-25

### Performance

- Blend glyphs with the x86-64-v3 variant when the cpu has it (#125) (render).

## 26.10.90 "Giga Fly" — 2026-10-25

### Added

- Draw emoji sequences through GSUB ligatures (#124) (render).

## 26.10.89 "Giga Fly" — 2026-10-25

### Fixed

- Keep the tab bar's border out of the padding above the grid (#123) (render).

## 26.10.88 "Giga Fly" — 2026-10-25

### Added

- Honor emoji and text variation selectors (#122) (render).

## 26.10.87 "Giga Fly" — 2026-10-25

### Added

- Lay out a delegated cgroup subtree per tab (#121) (cgroup).

## 26.10.86 "Giga Fly" — 2026-10-25

### Maintenance

- Record that the 26.10.23 parser regression is recovered (#120) (todo).

## 26.10.85 "Giga Fly" — 2026-10-25

### Added

- Allow fractional font sizes (#118) (config).

### Maintenance

- Record that the gpu work is claimed by vx9k (#119) (todo).

## 26.10.84 "Giga Fly" — 2026-10-25

### Fixed

- Extend only fully colored edge rows into the padding (#117) (render).

## 26.10.83 "Giga Fly" — 2026-10-25

### Maintenance

- Describe the delegated cgroup subtree still to do (#116) (todo).

## 26.10.82 "Giga Fly" — 2026-10-25

### Maintenance

- Require a SAFETY comment on every unsafe block (#115).

## 26.10.81 "Giga Fly" — 2026-10-25

### Maintenance

- Plan a complete clean-room PTY layer replacing portable-pty (#114) (todo).

## 26.10.80 "Giga Fly" — 2026-10-25

### Added

- Run each shell in its own systemd cgroup scope (#112) (pty).
- Give the terminal a higher CPU weight than its shells (#113) (pty).

## 26.10.79 "Giga Fly" — 2026-10-25

### Added

- Draw color emoji from CBDT bitmap fonts (#111) (fonts).

## 26.10.78 "Giga Fly" — 2026-10-25

### Added

- Draw characters the configured fonts lack from installed fonts (#110) (fonts).

## 26.10.77 "Giga Fly" — 2026-10-25

### Added

- Hold kitty animations still with reduce_motion (#107) (term).

### Fixed

- Show the selection clearly on light themes (#108) (ui).
- Grow the tab and status bars with large fonts (#109) (ui).

## 26.10.76 "Giga Fly" — 2026-10-25

### Added

- Zoom the terminal font with ctrl+= ctrl+- and ctrl+0 (#106) (input).

### Fixed

- Keep dimmed and accented tab text readable in every theme (#105) (ui).

## 26.10.75 "Giga Fly" — 2026-10-25

### Maintenance

- Plan the ui overhaul: centered chrome and one title line on linux (#104) (todo).
- Lay out the grid and window size in a tested ui::layout (#103) (app).

## 26.10.74 "Giga Fly" — 2026-10-25

### Fixed

- Follow display scale changes and draw chrome at one whole scale (#102) (app).

## 26.10.73 "Giga Fly" — 2026-10-25

### Maintenance

- Let marked window glue in src/app count for neither side of the ratio (#101).

## 26.10.72 "Giga Fly" — 2026-10-25

### Added

- Select a rectangle by holding alt while dragging (#100) (ui).

## 26.10.71 "Giga Fly" — 2026-10-25

### Maintenance

- Name the canvas pixel tests so the ratio check counts them (#99) (render).

## 26.10.70 "Giga Fly" — 2026-10-24

### Fixed

- Keep the selection on its text when full history evicts lines (#98) (ui).

## 26.10.69 "Giga Fly" — 2026-10-24

### Fixed

- Select nothing on a plain click and keep selecting past the edges (#97) (ui).

## 26.10.68 "Giga Fly" — 2026-10-24

### Fixed

- Let the keypad type its labels while numlock is honored (#94) (input).
- Give the entry a description so tic stops warning (#93) (terminfo).
- Percent-encode zsh hook bytes without the locale on musl (#95) (shell).

### Maintenance

- Update dependencies (#96) (deps).

## 26.10.67 "Giga Fly" — 2026-10-21

### Maintenance

- Update dependencies (#92) (deps).

## 26.10.66 "Giga Fly" — 2026-10-21

### Added

- Add nord and rose pine variants (#91) (theme).

## 26.10.65 "Giga Fly" — 2026-10-21

### Maintenance

- No need (#90).

## 26.10.64 "Giga Fly" — 2026-10-21

### Added

- Use new poopterm logo (#88) (branding).
- Add a renderer boundary (#46) (render).

### Maintenance

- Enforce built-in theme contrast (#89) (theme).

## 26.10.63 "Giga Fly" — 2026-10-21

### Maintenance

- Add project skills and settings for claude code (#87) (claude).

## 26.10.62 "Giga Fly" — 2026-10-20

### Maintenance

- Add an accessibility statement and a pull request template (#86).

## 26.10.61 "Giga Fly" — 2026-10-20

### Added

- Add an arch linux pkgbuild built and installed in ci (#82) (release).

### Fixed

- Match the pkgbuild to the 26.10.60 release (#84) (release).

### Maintenance

- Add claude code mention and review workflows (#83) (claude).
- Run opus 5.5 for organization members only (#85) (claude).

## 26.10.60 "Giga Fly" — 2026-10-20

### Added

- Build and attach a fedora rpm package (#81) (release).

## 26.10.59 "Giga Fly" — 2026-10-20

### Fixed

- Link musl builds dynamically so winit can load wayland (#79) (build).

## 26.10.58 "Giga Fly" — 2026-10-20

### Fixed

- Reuse hyperlink slots and end the open link on rejected targets (#78) (parser).

### Performance

- Compile the parse loop for x86-64-v3 and pick it at runtime (#77) (parser).

## 26.10.57 "Giga Fly" — 2026-10-20

### Maintenance

- Record memory footprint findings and plan (#76) (todo).

## 26.10.56 "Giga Fly" — 2026-10-20

### Performance

- Bolt the pgo build on linux (#75) (release).

## 26.10.55 "Giga Fly" — 2026-10-20

### Performance

- Ship profile-guided builds (#74) (release).

## 26.10.54 "Giga Fly" — 2026-10-20

### Performance

- Track each grid row's used extent (#73) (term).

## 26.10.53 "Giga Fly" — 2026-10-20

### Performance

- Read sgr groups in place (#72) (parser).

## 26.10.52 "Giga Fly" — 2026-10-20

### Added

- Mark continuation prompts with osc 133 k=s (#66) (shell).

## 26.10.51 "Giga Fly" — 2026-10-20

### Added

- Store and draw the SGR 58 underline color (#64) (render).

## 26.10.50 "Giga Fly" — 2026-10-20

### Fixed

- Match rv, xr and xm to what poopterm sends (#71) (terminfo).

## 26.10.49 "Giga Fly" — 2026-10-20

### Fixed

- Leave prompt marks to fish 4 and mark fish 3 command input (#70) (shell).

## 26.10.48 "Giga Fly" — 2026-10-20

### Added

- Send prompt clicks to shells that ask with click_events (#69) (shell).

## 26.10.47 "Giga Fly" — 2026-10-20

### Added

- Add reverse screen video and advertise flash (#62) (render).
- Advertise smm and rmm for the meta modes (#63) (terminfo).
- Report bash command lines with osc 133 cmdline_url (#68) (shell).

### Fixed

- Keep other hosts' osc 7 directories out of the local cwd (#65) (shell).

### Maintenance

- Install zsh and fish so the shell hook tests run on linux (#67).

## 26.10.46 "Giga Fly" — 2026-10-20

### Added

- Add reverse wraparound and advertise bw (#61) (parser).

## 26.10.45 "Giga Fly" — 2026-10-20

### Added

- Add HP memory lock and advertise meml, memu and TS (#60) (parser).

## 26.10.44 "Giga Fly" — 2026-10-20

### Added

- Advertise cursor color, overline and single insert (#59) (terminfo).

### Maintenance

- Align the docs with the parser, scrollback, sixel and inflate work (#58).

## 26.10.43 "Giga Fly" — 2026-10-20

### Performance

- Decode zlib streams with our own SIMD inflate (#57) (inflate).

## 26.10.42 "Giga Fly" — 2026-10-20

### Performance

- Decode into one flat buffer with alpha resolved up front (#56) (sixel).

## 26.10.41 "Giga Fly" — 2026-10-20

### Performance

- Join marks to clusters without ending the print segment (#55) (parser).

## 26.10.40 "Giga Fly" — 2026-10-20

### Performance

- Stream history rows past the cache (#54) (term).

## 26.10.39 "Giga Fly" — 2026-10-20

### Performance

- Build narrow and wide cell words once per segment (#53) (parser).

## 26.10.38 "Giga Fly" — 2026-10-20

### Performance

- Print narrow multibyte runs sixteen bytes at a time (#52) (parser).

## 26.10.37 "Giga Fly" — 2026-10-20

### Performance

- Rotate the screen rows by one in a single move on line feed (#51) (term).

## 26.10.36 "Giga Fly" — 2026-10-20

### Performance

- Answer repeated marks without making the extras table unique (#50) (term).

## 26.10.35 "Giga Fly" — 2026-10-20

### Performance

- Look widths up in a table generated from the unicode data (#49) (parser).

## 26.10.34 "Giga Fly" — 2026-10-20

### Performance

- Fill cells with 32-byte avx2 stores when the cpu has them (#48) (term).

## 26.10.33 "Giga Fly" — 2026-10-20

### Performance

- Blank only the written prefix of rows scrolled into history (#47) (term).

## 26.10.32 "Giga Fly" — 2026-10-20

### Performance

- Validate utf-8 with simd table lookups (#45) (vt).

## 26.10.31 "Giga Fly" — 2026-10-20

### Performance

- Store scrollback as trimmed rows in one ring buffer (#44) (term).

## 26.10.30 "Giga Fly" — 2026-10-19

### Maintenance

- Plan build and memory layout optimizations (#43).

## 26.10.29 "Giga Fly" — 2026-10-19

### Added

- Add decarm, decbkm and the meta, numlock and delete modes (#19) (input).

### Maintenance

- Check todo and the issues before starting work (#31).

## 26.10.28 "Giga Fly" — 2026-10-19

### Added

- Add modifyotherkeys and the key modifier options (#18) (input).

## 26.10.27 "Giga Fly" — 2026-10-19

### Added

- Add xtsave, xtrestore and the ansi modes (#16) (parser).

## 26.10.26 "Giga Fly" — 2026-10-19

### Added

- Add keypad application mode (#14) (input).

### Maintenance

- Attest the build provenance of released packages (#17) (release).

## 26.10.25 "Giga Fly" — 2026-10-19

### Fixed

- Attach the package before publishing the release (#15) (release).

## 26.10.24 "Giga Fly" — 2026-10-19

### Added

- Add hpr, vpr, hpb, vpb and decaln (#13) (parser).

## 26.10.23 "Giga Fly" — 2026-10-19

### Fixed

- Push each release to main once so merges cannot race it (#12) (release).

## 26.10.22 "Giga Fly" — 2026-10-19

### Maintenance

- Run only what needs freebsd in the vm and drop duplicate release builds (#11).

## 26.10.21 "Giga Fly" — 2026-10-19

### Breaking changes

- Relicense under the GNU AGPL, version 3 or later (#10).

## 26.10.20 "Giga Fly" — 2026-10-19

### Added

- Compact tabs and add amoled theme (#2) (tabs).

### Fixed

- Package the release commit so the deb carries the new version (#5) (release).
- Install a python with tomllib for the release scripts (#8) (release).

### Maintenance

- Count only the pull request's own lines in the 3:1 check (#6).
- Bump actions/upload-artifact from 4 to 7 (#4) (deps).
- Upload the pull request's debian package as the deb itself (#7).
- Add issue forms for bugs, rendering, compatibility, performance and features (#9).

## 26.10.19 "Giga Fly" — 2026-10-19

### Maintenance

- Require pull requests and 3:1 test-driven development (#1).

## 26.10.18 "Giga Fly" — 2026-10-19

### Added

- Draw box drawing, blocks, braille and powerline in code (render).

### Fixed

- Let wide glyphs spill into a blank cell or shrink to fit (render).
- Collapse the headers that old releases stacked up on save (config).

## 26.10.17 "Giga Fly" — 2026-10-19

### Added

- Add our own font engine, checked against fontdue (fonts).
- Draw glyphs with our own font engine instead of fontdue (fonts).

### Fixed

- Leave one-point contours out of glyph bounds (fonts).

### Performance

- Avoid floor and ceil library calls in the rasterizer (fonts).
- Solve for curve extremes only when a control point sticks out (fonts).
- Walk rows incrementally and drop NaN handling in line drawing (fonts).
- Decode TrueType points into one buffer (fonts).

### Maintenance

- Account for fontdue's own bugs in the installed-font sweep (fonts).
- Compare the font engine's speed with fontdue (fonts).

## 26.10.16 "Giga Fly" — 2026-10-19

### Maintenance

- Fix typo in Debian package description.

## 26.10.15 "Giga Fly" — 2026-10-19

### Added

- Add Debian APT package release (packaging).

### Fixed

- Provide control metadata for dependency scan (packaging).

## 26.10.14 "Giga Fly" — 2026-10-19

### Added

- Fill the window with the grid and extend edge colors (render).

## 26.10.13 "Giga Fly" — 2026-10-19

### Added

- Choose the terminal font from a list of installed fonts (settings).

### Fixed

- Count only text fonts with Latin letters as monospace (fonts).

### Maintenance

- Pin the third-party FreeBSD VM action to a commit.

## 26.10.12 "Giga Fly" — 2026-10-19

### Added

- Select the terminal font by family from a cached font database (fonts).

## 26.10.11 "Giga Fly" — 2026-10-19

### Fixed

- Fit the settings window on small screens again (ui).
- Parse XTVERSION replies in linear time (bench).

### Performance

- Redraw only changed rows with vertical tabs (render).

### Maintenance

- Run the release codename parser's tests (build).
- Switch to a project-tuned CodeQL setup (codeql).
- Add contributing guide, code of conduct and security policy.

## 26.10.10 "Giga Fly" — 2026-10-19

### Added

- Add configurable tabs and appearance controls.

### Fixed

- Move draw tests after implementation (ui).

## 26.10.9 "Giga Fly" — 2026-10-19

### Maintenance

- Create dependabot.yml.

## 26.10.8 "Giga Fly" — 2026-10-19

### Added

- Add a transparent huge page hint for large buffers (hugepages).

### Maintenance

- Drop the completed first-release item (todo).

## 26.10.7 "Giga Fly" — 2026-10-19

### Fixed

- Scroll back to the bottom when a key is typed (input).

## 26.10.6 "Giga Fly" — 2026-10-19

### Fixed

- Put the terminfo entry where ncurses looks for it (install).

### Performance

- Use mimalloc as the global allocator.

## 26.10.5 "Giga Fly" — 2026-10-19

### Performance

- Reuse the frozen frame's buffers across synchronized blocks (term).

### Maintenance

- Plan migrating the global allocator to mimalloc (todo).

## 26.10.4 "Giga Fly" — 2026-10-19

### Fixed

- Write changelog notes from the commits since the last tag (release).

### Performance

- Freeze a frame for synchronized output instead of cloning the terminal (term).
- Cache recent combining-mark lookups in front of the interner (term).

### Maintenance

- Start headless runs with a full scrollback.
- Fill in 26.10.1 to 26.10.3 from their commits (changelog).
- Replace outdated performance numbers with current measurements.

## 26.10.3 "Giga Fly" — 2026-10-19

### Maintenance

- Add profile-guided optimization to the Makefile.

## 26.10.2 "Giga Fly" — 2026-10-19

### Breaking changes

- Install system-wide only (install). scripts/install.sh no longer installs to ~/.local or honors XDG_DATA_HOME; remove an earlier per-user copy by hand.

### Added

- Support every xterm and kitty color sequence (parser).
- Support kitty's clipboard protocol and every OSC 52 selection (parser).
- Show OSC 9, OSC 777 and kitty OSC 99 desktop notifications (notifier).
- Track OSC 7501 program status and OSC 9;4 progress on tabs (parser).
- Support OSC 22 pointer shapes (parser).
- Support every OSC 133 marker and option (shell_integration).
- Support VS Code's OSC 633 and OSC 3008 contexts (parser).
- Support iTerm2's OSC 1337 commands besides images (parser).
- Support ConEmu's OSC 9 commands and xterm's icon name (parser).
- Support iTerm2's tab color and session status (parser).
- Show iTerm2 inline images (parser).
- Answer XTWINOPS size reports and keep a title stack (parser).
- Answer DECRQSS (parser).
- Answer XTGETTCAP from poopterm's terminfo entry (parser).
- Support kitty's text sizing protocol (parser).

### Performance

- Intern cell extras with one hash write and one lookup (term).
- Copy short ASCII titles without filtering each character (parser).

### Maintenance

- Forbid new crates and require complete escape sequence support.
- Describe the new escape sequences and the modules behind them.
- Add debugging and profiling profiles on top of release.

## 26.10.1 "Giga Fly" — 2026-10-19

### Added

- Finish desktop TODO follow-ups.
- XDG install and desktop integration.
- Replace vte with an in-house SIMD tokenizer (vt).
- Implement the kitty graphics protocol core (kitty).
- Layer images by z-index and keep partial redraws with images (render).
- Support unicode placeholders (kitty).
- Support animation frames, composition and playback (kitty).

### Fixed

- Leave exit-status colors to the shell.
- Use TERM=xterm-poopterm only when installed system-wide.
- Stop shell tests hanging the FreeBSD job (ci).
- Skip underline color arguments instead of applying them (sgr).
- Abort a chunked transmission when a delete command arrives (kitty).
- Keep images in the scrollback when text is written on screen (sixel).

### Performance

- Repair wide characters only at text segment edges (parser).
- Decode base64 payloads 16 characters at a time with SIMD (kitty).
- Append DCS payloads in bulk (sixel).

### Maintenance

- Keep kitty runs valid and name terminals reliably.
- TODO for 200 MB/s text, gigabit images and an own parser.
- Design for replacing vte with an in-house SIMD parser.
- One corpus of real-world workloads, headless and end to end.
- Adopt conventional commits.
- Make 200 MB/s a floor and images unbounded.
- Share bitmaps and placements between image protocols (graphics).
- Plan hand-written Vulkan and Metal renderers (todo).
- Plan replacing pty, width, inflate, font and present crates (todo).
- Plan automatic cgroups v2 per tab and huge pages (todo).
- Describe kitty graphics support, image tests and numbers.
- Keep interactive bash from stopping itself on FreeBSD (shell).
- Fill shared memory through a mapping (kitty).

## 26.10.0 "Giga Fly" — 2026-10-18

Poopterm now numbers releases `YEAR.MONTH.PATCH`, and each release gets a
codename.

### Performance

- Each tab parses its output on its own thread, and frames render from a snapshot, so the window stays responsive during floods: on a Unicode flood it now draws 29 frames per second instead of 2.
- Damage tracking: only changed rows are redrawn, scrolled lines are shifted instead of redrawn, and only changed pixels are copied to the window. Typing and small updates no longer repaint the whole window.
- While output arrives faster than it can be parsed, frames are capped at 30 per second, so rendering doesn't slow parsing down.
- End-to-end output throughput rose from 0.3 MB/s to about 100 MB/s for plain text (65 MB/s Unicode, 112 MB/s escape-heavy) on an i3-N305: a non-blocking PTY reader with backpressure, display-paced redraws, a row-based grid with 16-byte cells, batched ASCII printing and a portable-SIMD parser fast path.
- Glyph lookups use one fast hash per cell; shortcuts are no longer read from disk on every key press.

### Security

- Ctrl/Cmd-clicking a `file://` link no longer opens executables or launcher files (`.desktop`, `.command`, …); their paths are copied instead. Hovering a hyperlink shows its real target in the status bar.
- The shell hooks percent-encode every unsafe byte in directory names; a crafted directory name could inject terminal sequences before.

### Fixed

- Characters split across PTY reads could drop up to two following bytes (worked around a vte 0.15 bug).
- Pasting into a program that is busy writing output could freeze the window.
- Legacy (X10) mouse reports sent decimal digits instead of bytes; motion reported the wrong button.
- Alt+key now sends an ESC prefix, and Ctrl/Alt/Shift with arrows, Home/End and function keys use xterm modifier codes; Home/End honor application cursor mode.
- Pasted text can no longer end a bracketed paste early and run commands.
- Touchpad scrolling accumulates small movements; the wheel scrolls `less`, `man` and other alternate-screen programs.
- The zsh hooks failed on every prompt (`status` is read-only) and prefixed paths with `%23`; the bash command marker was placed before the prompt.
- The Settings "About" back button responded to clicks offset 24 px from where it is drawn.

### Added

- An `xterm-poopterm` terminfo entry, used as `TERM`. When it isn't installed, poopterm compiles a private copy with `tic` on first launch and falls back to `xterm-256color` without `tic`. `poopterm --print-terminfo` prints it, `make install` installs it, and the `term` config key overrides the choice.
- zsh and fish load the shell hooks automatically (`shell_integration = "off"` disables this). `ssh` from those shells uses `TERM=xterm-256color`.
- Jump to the previous or next prompt (Ctrl+Shift+Up/Down, Cmd+Up/Down on macOS; `shortcut_previous_prompt` and `shortcut_next_prompt`), "Copy last command output" in the command palette, exit-status marks beside prompts, and a tab marker plus attention request when a long command finishes unseen.
- Repeat character (REP), cursor shapes (DECSCUSR), focus reporting (mode 1004), XTVERSION and soft reset (DECSTR).
- Secondary device attributes (`CSI > c`) and DECRQM replies for supported modes.
- `scripts/bench-throughput.py`, a terminal-agnostic throughput benchmark.
- `AGENTS.md` and `docs/` for contributors and coding agents.

### Maintenance

- `main.rs` was split into small modules (`app`, `term`, `parser`, `pty`, `render`, `input`, `ui`, …) with unit tests beside the code.

## 0.43.0 — 2026-10-17

### Added

- Added selectable PNG, JPEG, and WebP terminal wallpapers with an adjustable strength control in Settings.
- Added optional zsh, Bash, and fish shell hooks for command markers and working-directory tracking.
- Added OSC 52 clipboard writes while leaving clipboard read requests unanswered.
- Added system Nerd Font and symbol-font fallback discovery for missing glyphs.

### Improved

- Cached resized wallpaper pixels and reused PTY reader buffers to reduce redraw and output-processing overhead.
- Optimized common SGR parsing and opaque Sixel image rendering.
- Preserved session startup when a previously saved working directory no longer exists.

## 0.42.0 — 2026-10-17

### Added

- Added a Custom theme with foreground, background, border, and ANSI color overrides; user-selected TrueType fonts with bundled glyph fallback.
- Added a searchable command palette, tab renaming, configurable keyboard shortcuts, and the About poopterm page in Settings.
- Added OSC 8 hyperlinks, local `file://` links, basic Unicode cluster retention, and initial Kitty keyboard protocol support.
- Added saved main-window opacity controls on macOS.
- Bundled Catppuccin's MIT license notice in `THIRD_PARTY_NOTICES`.

### Fixed

- Replaced the incomplete hand-rolled ANSI byte parser with vte's incremental parser and expanded SGR, OSC, and terminal mode handling.
- Improved terminal resize, alternate-screen, cursor, and color handling.
- Corrected Light/Dark theme labels and checked Catppuccin palette values against upstream.

## 0.4.0 — 2026-10-16

### Added

- Settings controls for font size (10–32), scrollback (0–100,000 lines), and session restore.
- Synchronized output (`CSI ? 2026 h/l`) with atomic state commits, mode queries, and a 250 ms recovery timeout.
- OSC 133 prompt, command, output, and exit-status tracking; the status bar reports running commands and exit codes when the shell emits these markers.
- Bounded Sixel decoding and rendering: RGB/HLS registers, repeats, transparency, raster attributes, aspect ratios, and cursor advancement.
- Colon-separated TrueColor, italic simulation, dim, underline, conceal, strikethrough, programmable tabs, and DEC line drawing.
- `--version` and `--help` without opening a GUI.
- macOS/Linux CI and immutable version tags/releases after successful checks on main.

### Fixed

- DEC private modes were incorrectly reading vte's ignore flag, breaking alternate screens, mouse tracking, bracketed paste, and cursor visibility.
- Deferred wrapping at the right margin, erase backgrounds, scroll regions, saved cursor attributes, and wide-character rendering/overwriting.
- Settings preserve unknown keys and comments, save atomically, and no longer duplicate headers.
- Terminal font changes no longer change toolbar typography.
- Search retains its selected match during output and accepts clipboard paste.
- Percent-encoded working directories and missing restored directories.

### Performance and maintenance

- Split terminal state, escape protocols, shell integration, Sixel, and configuration into modules.
- Bound pending PTY output and glyph caching; share scrollback and bitmap data across synchronized snapshots.
- Refresh search once per drawn frame; avoid redundant PTY flushes and reap closed child processes.
- Preserve user app icons and keep AUR packaging deferred.

### Limits

This is a growing emulator, not complete xterm compliance. Combining marks and basic ZWJ sequences stay with their base cell, but full grapheme shaping and complex-script shaping are not implemented. Kitty keyboard support covers the basic disambiguation mode rather than every progressive enhancement. Terminal image protocols other than Sixel remain unsupported. Sixel images are limited to 2,048 × 2,048 pixels, 1 MiB encoded data, 256 color registers, and 16 retained images within a 4-megapixel budget per screen. Images are discarded when their cells are overwritten or they scroll out of the visible region; image scrollback and partial bitmap erasure are not supported. Sixel aspect ratios are integer-rounded (1–10). The standard sixel display/scrolling mode 80 is supported. Shell integration requires shell-emitted OSC 133 markers; shell startup files are never modified automatically.
