---
name: meeting-the-test-ratio
description: Use when scripts/pr_check.py reports too few test lines for the production lines added, or when a change looks hard to cover three test lines per code line.
---

# Meeting the 3:1 test ratio

`scripts/pr_check.py` needs three added test lines per added production
line (non-blank, non-comment). The only exemption is marked window glue (see below), and padding is
rejected in review like any bad test. The ratio is a pressure to test
behavior you have not pinned yet, not a line count to reach.

## What counts as a test line

- `tests.rs`, `*_tests.rs`, `*_bench.rs`, anything under a `tests/` directory.
- The inline `#[cfg(test)] mod … {` **that closes the file**, and everything after it.
- Python: only files under `tests/`. `scripts/*.py` is production code.
- Other files (Markdown, YAML, TOML) count for neither side.

## Find real tests, in this order

1. **Edges and bounds.** Zero, one, the limit, the limit + 1, the last
   column and row, vector-width boundaries (15/16/17, 31/32/33 bytes),
   buffer ends, `u16::MAX` parameters.
2. **Failure paths.** Malformed input, truncated sequences, oversized
   payloads, invalid UTF-8. Untrusted input must stay bounded: assert the
   limit holds and nothing is replied.
3. **Regressions.** The exact input from the bug report.
4. **Differential cases.** Add fragments to the oracle tests:
   `PIECES` in `src/vt/tests.rs` (vte) and the fragments in
   `batched_printing_matches_per_character_printing` (`src/term/tests.rs`).
   They also gain random coverage.
5. **Mutation check.** Break each production line you added (flip a
   comparison, drop a branch, shift a bound) and run the tests. A mutant
   that survives is a missing test; write it. A line no mutant can expose
   does nothing observable: delete it, or comment why it exists.

## Still short?

The change is too big or too hard to test. Do one of:

- Split it into pull requests that each carry their tests.
- Pull the logic out of `App`/window code into a pure function taking plain
  data, which takes few lines to test exhaustively.
- Delete production code: simpler code needs fewer tests.

Only then, for the few lines in `src/app/` that a real window alone runs
(winit event arms, assigning what a pure function computed), mark a window
glue region as AGENTS.md shows: `// pr-check: window glue: <reason>` …
`// pr-check: end window glue`, at most 12 lines, no decisions inside.

## Red flags: this is padding

- Repeating an assertion with a different variable name.
- A loop or table whose only job is to add lines.
- Asserting what the compiler already guarantees, or a constant's value.
- Spreading one assertion over many lines for no readability gain.
- Tests that cannot fail when the new code is reverted.

Any of these means: delete it and go back to the list above.
