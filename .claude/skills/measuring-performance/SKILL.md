---
name: measuring-performance
description: Use when making or reviewing a change meant to make Poopterm faster (parser, SIMD, rendering, decoding, PTY), when a throughput regression is suspected, or before writing a perf(...) pull request title.
---

# Measuring performance

Performance work is measured, not guessed. A change without a reproducible
gain is dropped, however clever. Full details: `docs/performance.md`.

## Workflow

1. **Baseline first.** Build the unchanged code and keep the binary:
   `cargo build --release && cp target/release/poopterm /tmp/poopterm-before`.
2. **Pick the benchmark that isolates the change:**

| Changed | Benchmark |
|---------|-----------|
| Parser, terminal state | `poopterm --benchmark --only <workloads>` (headless) or `cargo test --release -- --ignored --nocapture parser_throughput` |
| Interactive paths (echo, prompts) | `poopterm --benchmark --kib 64 --read 16 --frame-every 64` |
| Rendering, damage | `highlight_rendering_throughput`; `--benchmark` with frames |
| Inflate | `inflate_throughput` |
| Fonts | `font_engine_speed`, `font_engine_stages` |
| End to end, in a window | `poopterm --benchmark --terminal`, run inside the window |
| PTY ceiling, reader and parser threads | `poopterm --benchmark --stages pty,pty-raw,pipeline,pipeline-raw` |

   `poopterm --benchmark --list` lists the workloads. Report every workload,
   not just the one that improved: a regression elsewhere counts.
3. **Alternate builds** A B B A A B …, at least three runs each, on the same
   machine, and report medians with min–max. Noise is a few percent.
4. **Under ~2%?** Also compare `perf stat -e instructions:u` for both
   builds; instruction counts barely vary between runs.
5. **Profile to find work**, not to prove gains:
   `cargo build --profile profiling && perf record -e cycles:u --
   ./target/profiling/poopterm --benchmark --only emoji && perf report --sort sym`
   (callgrind when perf is unavailable).

## Correctness comes first

A fast path must produce output identical to the slow path. Test it
differentially: SIMD against the portable kernel, batched printing against
per-character printing, our decoder against its oracle crate. Keep memory
bounded and never skip state to go faster.

## The pull request

Title `perf(scope): …`. The body has a before/after table per workload
(median, spread, machine, build profile, command line). TODO.md's
throughput numbers are updated if the change moves them.

## Common mistakes

| Mistake | Fix |
|---------|-----|
| One run of each build | Alternate, several runs, medians. |
| Comparing a debug or `debugging` build | `--release` (or `profiling` for profiles only). |
| Measuring only the workload you targeted | Run all; show all. |
| Keeping a change that "should" help but measures flat | Drop it. Give the numbers in the PR or issue, and delete the idea from TODO.md: it lists only work still wanted. |
| Comparing terminals at different window sizes | Same size, same session. |
