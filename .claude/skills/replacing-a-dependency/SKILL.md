---
name: replacing-a-dependency
description: Use when a task seems to need a new crate, when replacing an existing dependency (image and its PNG decoder, miniz_oxide, portable-pty, rfd, winit, softbuffer, libc) with in-house code, or when implementing a protocol, decoder or platform integration such as D-Bus, notifications or clipboard formats.
---

# Replacing a dependency

Poopterm adds no crates. The only deliberate exception is `mimalloc`.
Features are written in-house from specifications, and existing crates are
replaced over time (see `TODO.md`). "There is a crate for this" is never a
reason to add it.

## Clean room

Work from the specification (RFC 1951 for DEFLATE, the PNG spec, the
OpenType spec, the D-Bus spec, Unicode UAX #11/#29…). Never read or copy
another implementation's source, the crate you are replacing included.
Reading its public API docs to match behavior is fine.

## The crate becomes the oracle

The replaced crate stays as a dev-dependency (or is already one) and the
new code is tested **against it**, the way the repository already does:

| Ours | Oracle | Tests |
|------|--------|-------|
| `src/vt` | vte | `src/vt/tests.rs` |
| `src/inflate` | miniz_oxide | `src/inflate/*_tests.rs` |
| `src/fonts` | fontdue | `src/fonts/*_tests.rs` |
| `src/width.rs` | unicode-width | `src/term/width_tests.rs` |
| UTF-8 validator | `str::from_utf8` | `src/vt/utf8_tests.rs` |

Each needs:

1. **Random differential tests** from the seeded `bench::corpus::Rng`, so a
   failure reproduces from its seed. Shrink failing cases to a minimal input.
2. **Handwritten edges**: empty input, one byte, every length and code,
   buffer ends, vector-width boundaries.
3. **Malformed and hostile input**: truncated, damaged, oversized. Ours must
   reject or bound it without panicking, even where the oracle panics.
4. **Explicit limits** (`MAX_*`) on sizes from untrusted input.
5. **A benchmark** against the oracle (`*_bench.rs`, an ignored test), and a
   measured result. **REQUIRED SUB-SKILL:** measuring-performance.

## Moving code over

- Swap call sites only once the differential tests pass.
- Remove the crate from `[dependencies]` only when nothing but tests uses
  it; move it to `[dev-dependencies]` to keep the oracle.
- Update `THIRD_PARTY_NOTICES`, `TODO.md` and `docs/architecture.md`.
- Write the replacement in safe Rust. Where it must call C or the kernel,
  wrap each call in one small safe function and list it in
  `docs/unsafe.md`; never use `unsafe` to make the replacement faster.

## Red flags

- `cargo add` anywhere in your plan.
- "Just for now, we will replace it later."
- "I looked at how the crate does it."
- Tests that compare against expected values you computed by hand only.

All of these mean: stop and go back to the specification and the oracle.
