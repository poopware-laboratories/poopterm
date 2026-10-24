---
name: checking-rendering-changes
description: Use when changing anything Poopterm draws (src/render, src/app/canvas*, draw*, themes, fonts, cursor, tabs, settings UI, images) or row damage and partial redraws, before claiming the change looks right.
---

# Checking rendering changes

Unit tests cannot see pixels. A rendering change is verified three ways:
pixel tests, a screenshot comparison, and partial against full redraws.
Full process: `docs/testing.md#visual-checks`.

## 1. Pixel tests

`src/app/canvas_pixels_tests.rs` checks that partial redraws equal full redraws
pixel for pixel, and exact pixels for layering and placeholders. Add a case
for what you changed (highlights, scrolling, images, animation frames).
Drawing decisions belong in pure functions (`render::damage`, layout in
`ui::`) with ordinary unit tests.

## 2. Screenshot before and after

1. A fake shell that `sleep 1`s (the window must reach its final size),
   then prints a fixed sample: styled text, 256 colors, wide and combining
   characters, box drawing, and whatever you changed. Then it sleeps.
2. A throwaway config, so your settings and sessions stay untouched:
   ```sh
   POOPTERM_CONFIG=/tmp/cfg/config.toml SHELL=/tmp/fake-shell.sh target/release/poopterm
   ```
3. On X11: `import -window <id> before.png`, the same for the new build,
   then `compare -metric AE before.png after.png diff.png`. Zero differing
   pixels means visually neutral; otherwise look at `diff.png` and make
   sure every difference is intended.

No screen (cloud session, Wayland without X)? Use a temporary ignored test
that feeds the bytes to a `Term`, draws the `Frame` with the helpers in
`canvas_pixels_tests.rs`, and saves a PNG with `image::save_buffer`. Look at the
PNG, then remove the test.

## 3. Partial against full redraws

Make the fake shell print in steps (scrolling, cursor moves, clears), and
screenshot the same build with and without `POOPTERM_FULL_REDRAW=1`. Any
difference is a damage-tracking bug.

## Also

- Run new UI paths once in a debug build: it panics on a double terminal
  lock instead of deadlocking.
- Check every built-in theme, light ones included, and a scale factor
  other than 1 (see `ACCESSIBILITY.md`).
- Put the before/after screenshots in the pull request.
