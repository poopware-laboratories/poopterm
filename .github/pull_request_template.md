<!--
Title: a Conventional Commit, e.g. `fix(render): close the arc gap`.
It becomes the squashed commit on main and its changelog line.
The full rules are in AGENTS.md; CONTRIBUTING.md is the short version.
-->

## Why

<!-- The problem this solves or the behavior it adds. Not what the diff does line by line. -->

Closes #

## How it was checked

<!--
Delete what does not apply:
- Performance: before and after numbers from the same machine (docs/performance.md).
- Rendering: screenshots before and after, and partial redraws compared
  with POOPTERM_FULL_REDRAW=1 (docs/testing.md#visual-checks).
- Escape sequences: the exact bytes, and the spec section they follow.
-->

## Checklist

- [ ] I saw each new test fail for the expected reason before making it pass.
- [ ] `python3 scripts/pr_check.py --base origin/main --title "…"` passes (3 test lines per production line, no padding).
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.
- [ ] Platform-gated code is checked with clippy for the other targets (macOS, FreeBSD), or there is none.
- [ ] No new crates. Replacements of existing ones are tested against the crate they replace.
- [ ] No new `unsafe`, or the pull request explains why no safe formulation exists (never for speed) and `docs/unsafe.md` lists it.
- [ ] Input from programs stays bounded, and nothing answers a query that leaks data.
- [ ] Existing `config.toml` keys still work, and unknown keys survive a save.
- [ ] README, docs and `TODO.md` are updated where this changes what they say.

### Accessibility

For changes to what users see or do (see [ACCESSIBILITY.md](../ACCESSIBILITY.md)):

- [ ] Every new action works from the keyboard and can be bound to a shortcut.
- [ ] Color is not the only way meaning is shown, and new colors are readable in every built-in theme, light ones included.
- [ ] Nothing new blinks, flashes or animates on its own.
- [ ] New UI scales with the font size and the display scale factor.
- [ ] Not applicable: this changes nothing users see or do.
