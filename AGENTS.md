# AGENTS.md

Instructions for AI coding agents (any model or tool) and for humans working
on Poopterm. This file is the single source of truth; `CLAUDE.md` and any
other agent-specific files only point here. The Claude Code skills in
`.claude/skills/` turn these rules into per-task checklists (a change, an
escape sequence, a benchmark, a crate replacement, a rendering check, a pull
request to drive); when they disagree with this file, this file wins and the
skill is fixed.

Poopterm is a software-rendered terminal emulator written in Rust: winit for
windows, softbuffer for pixels, portable-pty for shells, its own font engine
(`src/fonts`, tested against fontdue), and its own SIMD escape-sequence
tokenizer (`src/vt`), tested against vte.

## Quick reference

```sh
cargo test                              # unit + emulation tests (fast, no window)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release                   # binary: target/release/poopterm
cargo build --profile debugging         # release speed with debug info, assertions, overflow checks, backtraces
cargo build --profile profiling         # release plus symbols, for perf and callgrind
cargo test --release -- --ignored --nocapture parser_throughput   # parser benchmark
```

- Toolchain: **nightly** (pinned by `rust-toolchain.toml`) for `portable_simd`.
- Formatting: hard tabs, width 100 (`rustfmt.toml`). Always run `cargo fmt`.
- Linux build deps: `pkg-config libx11-dev libxkbcommon-dev libwayland-dev`.
- CI (`.github/workflows/ci.yml`) runs fmt, test and clippy (including
  clippy for FreeBSD) on Linux and macOS, a release build on macOS and in the
  Debian and Fedora package jobs, and the tests and a debug build in a FreeBSD VM. All
  must pass.
- **Platform-gated code** (`#[cfg(target_os = ...)]`) is not compiled on your
  host. Before committing such changes, check the other targets too:
  `rustup target add aarch64-apple-darwin x86_64-unknown-freebsd`, then
  `cargo clippy --target <triple> --all-targets -- -D warnings` for each.

## Before you start

Check [`TODO.md`](TODO.md) and the open
[issues](https://github.com/poopterm-labs/poopterm/issues) first. Someone may
already be working on the change, an issue may set out how it should be
done, or a higher-priority item may come first. Comment on an issue to claim
it, and link the issue from your pull request (`Closes #N`).

## Workflow: 3:1 test-driven development

Every behavior change and bug fix follows red → green → refactor:

1. **Red.** Write a test that describes the desired behavior. Run it and
   confirm it fails *for the expected reason* (wrong output, or the API not
   existing yet), not because of a typo.
2. **Green.** Write the smallest change that makes it pass.
3. **Refactor.** Clean up while the whole suite stays green.
4. Run the full checks (test, clippy, fmt) before pushing.

A test that passes the first time you run it proves nothing; adjust it until
you have seen it fail. Pure refactors keep existing tests green and add
characterization tests for code that had none. See
[`docs/testing.md`](docs/testing.md) for where tests live and how to write
emulation tests.

**The 3:1 rule.** Every pull request adds at least **three lines of tests
for each line of production code**. `scripts/pr_check.py` enforces it in CI
and fails the pull request otherwise; run it locally before you open one:

```sh
python3 scripts/pr_check.py --base origin/main --title "fix(render): close the arc gap"
```

- It counts added lines that are neither blank nor comments. Test lines are
  those in `tests.rs`, `*_tests.rs`, `*_bench.rs`, anything under a
  `tests/` directory (including `tests/test_*.py`), and the inline
  `#[cfg(test)] mod … { }` that closes a source file. Everything else in
  `.rs` and `.py` files is production code; other files are not counted.
- Pull requests without production code (docs, CI, dependency bumps) pass.
- Meet the ratio with tests that pin real behavior: edge cases, bounds,
  failure paths, regressions, differential cases. Never pad (duplicate
  assertions, filler, or loops that only add lines); reviewers reject
  padding like any other bad test.
- The one exemption is **window glue**: lines in `src/app/` that only a
  real window runs (a winit event arm, assigning sizes a pure function
  computed), so no test can reach them. Mark them, with the reason:

  ```rust
  // pr-check: window glue: winit sends this when the output's scale changes
  WindowEvent::ScaleFactorChanged { .. } => self.apply_scale(),
  // pr-check: end window glue
  ```

  Marked lines count for neither side and `pr_check.py` reports how many
  there are. A region holds at most 12 lines and must sit in `src/app/`.
  Use it only after moving every decision into a tested pure function;
  logic inside a glue region is rejected in review.
- When a change cannot meet the ratio cheaply, that usually means it is too
  big or too hard to test. Split it, or make the logic pure and testable.

Performance work is measured, not guessed: benchmark before and after (see
[`docs/performance.md`](docs/performance.md)) and drop changes that do not
show a reproducible gain.

## Architecture in one screen

```
PTY reader thread ──► pty::OutputQueue ──► parser thread (pty::pump) ──► term::SharedTerm
                        (backpressure)        parser::Parser (vt + SIMD)      (Mutex<Term>)
                                                   │ wake once per batch          │
                                                   ▼                              │ snapshot
input events ──► App (UI thread) ──► input::{keys,mouse,paste} ──► pty::PtyWriter ──► shell
                     │                                                            │
                     └─ app::FramePacer ──► app::draw ◄── term::Frame ◄───────────┘
                          (30 fps in floods)    └─ render::damage + app::canvas ──► softbuffer
```

| Path | Responsibility |
|------|----------------|
| `src/term/` | Terminal state: `Grid` (one `Vec<Cell>` per row), scrollback, cursor, modes, Sixel placement, synchronized output. `Cell` is 16 bytes; combining marks and hyperlinks live in `term::extras`. `SharedTerm` is the lock shared with the parser thread; `Frame` is the copy the renderer draws. |
| `src/vt/` | Escape-sequence tokenizer: the DEC state machine with SIMD scans, handing text, CSI/ESC/OSC/DCS/APC events to a `vt::Handler`. vte is only a test oracle. |
| `src/parser/` | Applies `vt` events to `Term` (`impl Handler for Term`); `csi`, `esc`, `osc`, `sgr`, `print` apply each family. |
| `src/pty/` | Shell process, reader and parser threads (`pump`), `OutputQueue`, non-blocking `PtyWriter`. |
| `src/input/` | Pure encoders: keys, mouse reports, paste, scroll, shortcuts. |
| `src/graphics/` | Image data shared by Sixel and the kitty graphics protocol: `Bitmap`, `Placement`, streaming base64. `graphics::kitty` parses commands, receives chunked transfers, decodes and stores images and animation frames; `term/kitty*.rs` applies commands to `Term`. |
| `src/render/` | Pure drawing: `Painter`, `FontSet` glyph cache, grid renderer, row damage, SIMD blending. |
| `src/ui/` | Pure UI models: selection, search, tab layout. |
| `src/app/` | winit event loop and glue. `App` methods are split by concern (`keyboard`, `mouse`, `tabs`, `settings`, `draw`, …). |
| `src/inflate/` | Our own zlib/DEFLATE decoder for kitty `o=z` payloads, tested against `miniz_oxide`. |
| Top level | `config`, `theme`, `fonts`, `width` (generated width table), `clipboard`, `session_store`, `shell_integration`, `sixel`, `wallpaper`, `platform`. |

Details: [`docs/architecture.md`](docs/architecture.md).

## Rules of thumb

- **Keep files small and single-purpose.** Prefer a new module over growing
  a file past roughly 400 lines. Each file starts with a `//!` line saying
  what it is for.
- **Keep logic pure and testable.** Put encoding, layout and state changes in
  functions that take plain data (see `input::keys::encode_key`,
  `ui::tabs::tab_layout`, `app::settings::settings_target`). `App` methods
  should mostly route events to those functions.
- **Never block the UI thread** on the PTY. Write through `Session::write`;
  the parser thread does the reading and parsing.
- **Hold the terminal lock briefly, and never twice.** `Session::term()`
  returns a guard; drop it before calling anything that may lock again
  (debug builds panic instead of deadlocking). Watch for temporaries:
  `let x = a(self.active_term().y) && self.b()` keeps the guard alive for
  the whole statement. Render from a `term::Frame`, never under the lock.
- **The tokenizer must agree with vte.** Any change to `src/vt` must keep
  the differential tests in `vt/tests.rs` green, and any change to
  `Term::print_ascii` / `print_text` must keep
  `batched_printing_matches_per_character_printing` green; extend their
  fragment lists when you add new cases.
- **Bounds on untrusted input.** Escape sequences come from arbitrary
  programs. Keep explicit limits (Sixel size, OSC 52 size, hyperlink count,
  titles) and never answer requests that leak data (OSC 52 reads are ignored
  on purpose).
- **`unsafe` only where no safe API exists** (SIMD intrinsics, `libc`
  calls, proven-valid conversions on hot paths), in the smallest block that
  needs it, behind a safe function. Every block starts with a
  `// SAFETY:` comment saying why it is sound; clippy's
  `undocumented_unsafe_blocks` and rustc's `unsafe_op_in_unsafe_fn` are
  denied in `Cargo.toml`, so CI rejects one without it. Every block is
  listed, with its reasoning, in [`docs/UNSAFE.md`](docs/UNSAFE.md); update
  it in the same pull request.
- **Config compatibility.** `config.toml` keys are user-facing; keep old
  names working (see `ThemeKind::parse`) and preserve unknown keys on save.
- Match surrounding style: comment density, naming, and the existing `pub(crate)`
  / `pub(super)` visibility patterns.
- **No new crates; write our own.** Implement features in-house, from the
  specifications (clean room: never read or copy another implementation's
  source), so we keep fine-grained control over speed, limits and security.
  This holds for protocols, platform integration (notifications, D-Bus,
  clipboard formats) and decoders alike. Existing dependencies are being
  replaced over time (see `TODO.md`); never add one, and test each
  replacement differentially against the crate it replaces.
  The one deliberate exception is `mimalloc`, the global allocator
  (`src/main.rs`): system allocators, musl's above all, cost throughput.
- **Standards compliance.** Implement escape sequences completely, as their
  specifications define them (xterm ctlseqs, kitty, iTerm2, OSC 133/7501…),
  including queries and replies, with the bounds above. Unsupported or
  unknown sequences are ignored silently, never echoed.

## Branches, pull requests and commits

`main` is protected: no direct pushes, force-pushes or deletion. Every
change, however small, goes through a branch and a pull request.

1. **Branch from an up-to-date `main`:**
   `git switch main && git pull --rebase && git switch -c fix/arc-gap`.
   Name branches `type/short-topic` after the Conventional Commits type.
2. **Commit in small steps** on the branch (the red and green steps may be
   separate commits). To pick up new work from `main`, rebase:
   `git pull --rebase origin main`, then `git push --force-with-lease`.
   Force-push only to your own branch.
3. **Open a pull request** with `gh pr create`. One concern per pull
   request; keep it small. The **title is a Conventional Commit** (see
   below): it becomes the squashed commit on `main` and its changelog line.
   The body explains *why*, links the issue, and carries the evidence:
   benchmark numbers for performance work, screenshots for rendering
   changes. Add no tool footers ("Generated with …") or session links.
4. **Required checks** must all pass: CI (Linux, macOS, FreeBSD, Debian
   package), CodeQL, and the pull request rules (title and 3:1 ratio). No
   approval is required; ask for review when a second pair of eyes helps.
5. **Merge by squashing** once green: `gh pr merge --squash --delete-branch`.
   Squash is the only merge method; history on `main` stays linear, one
   commit per pull request.

CI runs only for pull requests and for `main`; pushes to other branches run
nothing until they have a pull request. Releases run only on `main`, after
a merge.

- **Titles and commits follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/):**
  `type(scope)!: summary`, with the scope optional, and the summary in lower
  case, imperative mood, and without a final period. Types: `feat`, `fix`,
  `perf`, `refactor`, `docs`, `test`, `bench`, `build`, `ci`, `style`,
  `chore`, `revert`. Scopes name the area touched, usually a module
  (`parser`, `vt`, `kitty`, `render`, `pty`, `input`, `bench`, `release`).
  Breaking changes get `!` and a `BREAKING CHANGE:` footer. The body explains
  *why*. Add no session links or other tool trailers; a `Co-Authored-By:`
  footer is fine. CI checks pull request titles; branch commits should
  follow the same form.
- **Versioning:** `YEAR.MONTH.PATCH` (two-digit year, month without a leading zero, e.g. 26.1.0 for January), plus a random
  codename per release (26.10.0 is "Giga Fly"). Put the codename in
  `[package.metadata.poopterm] codename` in `Cargo.toml` and in the
  changelog heading (`## 26.10.0 "Giga Fly" — date`); CI uses it in the
  release title. Patch releases keep their release's codename.
- After fully green CI on `main`, CI increments the patch or starts a new
  month at `YY.M.0` with a new random codename. It commits `Cargo.toml`,
  `Cargo.lock`, `CHANGELOG.md` and `Formula/poopterm.rb` together and
  atomically pushes the commit and tag only if `main` still matches the
  checked revision: one push per release, so merges cannot race it. If `main`
  moved, the run skips and the newer commit's run releases. Then it builds
  the Debian and RPM packages from the tag, attaches them to a draft release and only
  then publishes it, so releases can be immutable; a rerun resumes without
  pushing again. The changelog entry is built from
  the Conventional Commits since the last tag (`feat` → Added, `fix` → Fixed,
  `perf` → Performance, breaking changes first, the rest under Maintenance),
  so write pull request titles that read well there. The release job pushes
  with a write deploy key (secret `RELEASE_DEPLOY_KEY`), the only actor that
  may bypass the protection of `main`; its commits carry `[skip ci]` so they
  do not release again. Do not bump versions
  manually for normal work. Release logic lives in `scripts/release.py` and
  `scripts/release.sh`; test it with `python3 -m unittest discover -s tests`.

## Verifying UI changes

Unit tests cannot see pixels. For rendering changes, run the app
(`cargo run --release`) and look, or compare screenshots of a fixed screen
before and after, and partial redraws against `POOPTERM_FULL_REDRAW=1`; the process is described in
[`docs/testing.md`](docs/testing.md#visual-checks).
