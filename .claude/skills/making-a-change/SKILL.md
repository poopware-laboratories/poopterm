---
name: making-a-change
description: Use when starting any code change, bug fix or feature in Poopterm, and again before committing, pushing or opening a pull request.
---

# Making a change in Poopterm

AGENTS.md is the rulebook; this is the order to apply it in. Every step is
required. Skipping one is how pull requests go red.

## 1. Before writing code

- Read `TODO.md` and search the open issues for the topic. Someone may own
  it, or an issue may say how it must be done.
- Branch from an up-to-date `main`: `git switch main && git pull --rebase &&
  git switch -c type/short-topic` (`fix/arc-gap`, `perf/sixel-bands`).
- Find the module from the table in AGENTS.md ("Architecture in one
  screen") and the suite from `docs/testing.md` ("Where tests go").

## 2. Red, green, refactor

1. Write the test first. Run only it (`cargo test name`) and read the
   failure: it must fail for the expected reason (wrong value, missing
   API), never a typo or a panic in setup.
2. Write the smallest change that passes. Keep the logic in a pure
   function that takes plain data; `App` methods only route.
3. Refactor with the whole suite green.

A test that passed the first time proves nothing: break the code on purpose
(flip a comparison, drop a check, shift a bound by one) and watch it fail.

## 3. Checks, in this order

```sh
cargo fmt                                       # hard tabs, width 100
cargo clippy --all-targets -- -D warnings
cargo test
python3 -m unittest discover -s tests           # when scripts/ or workflows changed
python3 scripts/pr_check.py --base origin/main --title "type(scope): summary"
```

Touched `#[cfg(target_os = ...)]` code? It is not compiled on your host:

```sh
rustup target add aarch64-apple-darwin x86_64-unknown-freebsd
cargo clippy --target x86_64-unknown-freebsd --all-targets -- -D warnings
cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings
```

(The macOS target may fail to link without an SDK; clippy errors in our
code are what count.)

`pr_check.py` failing on the ratio? **REQUIRED SUB-SKILL:** meeting-the-test-ratio.

## 4. Commit and pull request

- Commits and the PR title are Conventional Commits: `type(scope): summary`,
  lower case, imperative, no final period. The title becomes the squashed
  commit and its changelog line (`feat` → Added, `fix` → Fixed, `perf` →
  Performance), so write it for a reader of the changelog.
- One concern per pull request. Link the issue: `Closes #N`.
- Fill in `.github/pull_request_template.md`: *Why*, *How it was checked*
  (benchmarks, screenshots, exact bytes and spec section), the checklist,
  and the accessibility section for anything users see or do.
- Never bump versions, edit `CHANGELOG.md` releases or `Formula/` by hand:
  the release job does that.

## Related skills

| Change | Skill |
|--------|-------|
| Escape sequence (CSI, OSC, DCS, APC, ESC) | implementing-escape-sequences |
| Anything claimed to be faster | measuring-performance |
| Replacing a crate, or anything new that a crate would do | replacing-a-dependency |
| Drawing, colors, layout, damage | checking-rendering-changes |
| Driving an open PR to green | steward |

## Common mistakes

| Mistake | Fix |
|---------|-----|
| Holding the `Session::term()` guard across a call that locks again | Bind what you need, drop the guard, then call. Debug builds panic. |
| Growing a file past ~400 lines | New module, with a `//!` line saying what it is for. |
| Inline `mod tests` that is not at the end of the file | pr_check only counts the `#[cfg(test)] mod` that closes the file. |
| Adding a crate | Never. Write it; see replacing-a-dependency. |
| Reaching for `unsafe` (casts, `_unchecked`, raw pointers, intrinsics) | Write it safe and make the safe code fast. `unsafe` is only for calls into C and the kernel; see AGENTS.md. |
| Renaming a `config.toml` key | Keep the old name parsing (see `ThemeKind::parse`). |
