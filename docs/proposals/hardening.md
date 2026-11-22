# Hardening notes (`local/hardened`)

What was hardened on top of `local/maximized`, and what it cost.

## Done

- **Whole-terminal fuzzing** (`src/term/fuzz_tests.rs`). Random streams
  built from fragments of every sequence family (text and invalid UTF-8,
  C0 controls, CSI with hostile parameters, DEC modes, OSC of every number
  handled, Sixel, kitty graphics, DCS requests, ESC sequences, hyperlinks,
  XTWINOPS), cut at random bytes, on terminals of random sizes resized as
  they go. Nothing may panic, and the cursor, rows and frames stay in
  bounds. 400 streams run with `cargo test`; `fuzz_long` runs 128,000 under
  the `debugging` profile (release speed with overflow checks).
- **Found and fixed:** an `rgb:` color channel of 8 or more hex digits made
  `parse_color` shift by 32 bits or more before checking the digits.
  Release builds masked the shift and returned `None` (no crash);
  `debugging` builds and tests panicked. The digits are now checked first.
- **Hardened runtime** on the macOS app (`codesign --options runtime`):
  `DYLD_*` variables and unsigned libraries can no longer be injected.
- Already in place and kept: `panic = "abort"`, fat LTO, stripped
  binaries, trimmed paths, `unsafe_code` denied outside the listed places,
  the security fixes of #372–#374. Linux builds get PIE and full RELRO from
  Rust's defaults.

## Cost

None measurable. `poopterm --benchmark --repeat 3`, Apple Silicon,
release, alternating preview / hardened / hardened / preview (overall
MB/s, parse and with rendering):

| Build | parse | +render |
|---|---|---|
| preview | 437.4 | 405.2 |
| hardened | 436.1 | 403.9 |
| hardened | 435.5 | 403.5 |
| preview | 428.8 | 401.4 |

The hardening is tests and signing; the only code change (checking digits
before scaling) is on a cold path.
