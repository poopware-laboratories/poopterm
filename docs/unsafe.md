# Unsafe code

Poopterm is written in safe Rust except for a few small places where no safe
API exists or where the safe one costs measurable throughput on a hot path.
This file lists every one of them: what it does, why it is sound, why it is
not safe code, and what tests it. Update it in the same pull request that
adds, moves or removes an `unsafe` block.

## The rules

From [`AGENTS.md`](../AGENTS.md):

- `unsafe` is allowed only where no safe API exists: SIMD intrinsics, `libc`
  calls, and conversions proven valid on hot paths.
- Keep it in the smallest block that needs it, behind a safe function.
- Every block starts with a `// SAFETY:` comment that says why it is sound.

The compiler enforces the comments: `Cargo.toml` denies clippy's
`undocumented_unsafe_blocks` and rustc's `unsafe_op_in_unsafe_fn`. An
`unsafe fn` must still wrap each unsafe operation inside it in its own
commented block, and documents what callers must ensure under `# Safety`.

These have no `unsafe`: the escape-sequence state machine's logic, the
terminal model apart from the cell layout below, the font engine
(`src/fonts`), our inflate (`src/inflate`), the kitty and Sixel decoders,
input encoding and the UI. Dependencies have their own `unsafe` (winit,
softbuffer, portable-pty, mimalloc, `image`); replacing them (see
[`TODO.md`](../TODO.md)) moves that code here, under these rules.

## Why there is any at all

- **SIMD on the hot paths.** Throughput is a goal of the project (at least
  200 MB/s on every payload). Portable SIMD (`std::simd`) is safe and is
  used wherever it is fast enough. Where an x86 instruction has no portable
  equivalent (non-temporal stores, `vpshufb` lookups in UTF-8 validation)
  or the AVX2 build of a function is measurably faster, we call intrinsics
  or compile a second copy of the function for the CPU's features. Both are
  `unsafe` by language rule.
- **Not checking twice.** Some data is already proven valid by an earlier
  check on the same path: bytes validated as UTF-8, a character decoded by
  SIMD from valid UTF-8. Checking again costs throughput for nothing, so
  those few conversions skip the check.
- **The operating system.** A few calls have no safe wrapper in `std`:
  `getuid`, `gethostname`, POSIX shared memory, `mmap`, `madvise`, and one
  Objective-C message on macOS. Since we write our own code instead of
  adding crates, those calls are ours too.

## Inventory

Paths are relative to `src/`. Test-only blocks are listed at the end.

### Functions compiled for x86-64-v3

Hot functions are compiled twice: once for the baseline x86-64 target, and
once with `#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,fma")]`.
`crate::cpu::has_v3()` checks those features at runtime (the standard
library caches the detection) and picks the copy. Calling a
`target_feature` function is `unsafe`, because running it on a CPU without
the features is undefined behavior.

| Where | What |
|-------|------|
| `vt/mod.rs` `Parser::advance` → `advance_v3` | The tokenizer, the hottest loop. |
| `parser/print.rs` `print_text` → `print_text_v3` | Writing text into cells. |
| `render/painter.rs` `glyph` → `glyph_v3` | Blending glyph coverage into the frame. |
| `term/mod.rs` `Cell::fill` → `term/fill.rs` `fill` (AVX2) | Filling runs of cells: erases, clears, new rows. |
| `vt/utf8.rs` `is_valid` → `x86::is_valid` (AVX2) | UTF-8 validation of printable runs. |

- **Sound because** each call site checks `has_v3()` (or AVX2 alone)
  first, in the same function, and the `unsafe fn`s are private to their
  module or crate.
- **Tested** by differential tests that run both copies on the same input
  and require identical results: `vt/tests.rs` (the tokenizer, also
  against vte), `parser/dispatch_tests.rs` (parsing and printing),
  `painter.rs` `both_variants` (glyph pixels), `term/fill_tests.rs` and
  `vt/utf8_tests.rs` (every variant against `std::str::from_utf8`).
- **Measured:** the v3 copies were kept only where they win (#125 for
  glyph blending); a v3 base64 decoder measured slower and was dropped
  (#36).

### Intrinsics with raw pointers

| Where | What | Why it is sound |
|-------|------|-----------------|
| `term/fill.rs` | `_mm256_storeu_si256` (two cells) and `_mm_store_si128` (the last one) | The pointers come from `&mut [Cell]` chunks of exactly 32 and 16 bytes; `Cell` is 16-byte aligned. |
| `term/scrollback.rs` `stream_copy` | `_mm_stream_si128` copies rows into history without pulling them into the cache | Cells are 16 bytes and 16-byte aligned; SSE2 is part of every x86-64 CPU. |
| `term/scrollback.rs` `publish_streamed` | `_mm_sfence` after the streamed stores | Streaming stores are weakly ordered. The fence orders them before the unlock that publishes the rows to the render thread; without it the renderer could read stale cells. SSE is part of x86-64. |
| `vt/utf8.rs` | `_mm_loadu_si128` and `_mm256_loadu_si256` loads | Each reads from an array or chunk of at least the loaded size; the tail is copied into a zero-padded block first. |

There is no safe intrinsic for non-temporal stores, and the history copy
is the largest `memmove` in text floods (see
[`performance.md`](performance.md#where-the-time-goes-now)).

### Reinterpreting cells

`Cell` is `#[repr(C, align(16))]`: `ch` (4 bytes), `fg` (4), `bg` (4),
`attrs` (1), `width` (1), `extra` (2). That is exactly 16 bytes with no
padding, so every byte is initialized and the cell can be viewed as vector
words. `term/tests.rs` asserts the size, and `transmute` refuses to compile
if it changes.

| Where | View |
|-------|------|
| `term/mod.rs` `Cell::bits` | The cell as one `u128` (equal exactly when the cells are). |
| `term/mod.rs` `Cell::words` | The cell as `u32x4`, the character in lane 0. |
| `term/mod.rs` `Cell::words_mut` | `&mut [Cell]` as `&mut [u32x4]`, to store whole cells with one vector store. |
| `term/mod.rs` `Cell::fill` (portable path) | `&mut [Cell]` as `&mut [u64x2]`. |
| `term/scrollback.rs` `trimmed_len` | `&[Cell]` as `&[u64]`, to find trailing blank cells four at a time. |

The read-only views are always sound: any bit pattern is a valid integer.
The mutable views can write any bits, but a `char` has invalid values
(surrogates, above U+10FFFF), so writers must keep lane 0 a valid `char`:

- `Cell::fill` writes copies of a real `Cell`, so this holds.
- `Cell::words_mut` is an `unsafe fn`: its `# Safety` section makes each
  caller keep lane 0 a valid `char`. Its only production caller,
  `parser/print.rs`, stores `spacer` (a whole cell), `with_char` of a
  `char`, or characters decoded from validated UTF-8.

The layout these views rely on (16 bytes, 16-byte alignment, no padding) is
checked at compile time by a `const` assert next to `Cell`.

### Conversions proven valid earlier

| Where | Conversion | Proven by |
|-------|-----------|-----------|
| `vt/mod.rs` `Handler::print_ascii` | `str::from_utf8_unchecked` | The tokenizer only passes `0x20..=0x7e` here, which is ASCII and so UTF-8. |
| `vt/mod.rs` `text` (2 blocks) | `str::from_utf8_unchecked` | The run, or the prefix `from_utf8` reported valid, was validated just before. |
| `vt/mod.rs` (character split across reads) | `str::from_utf8_unchecked` | `from_utf8` validated at least this character. |
| `parser/narrow.rs` (2 blocks) | `char::from_u32_unchecked` | The SIMD decoder only reads lanes that end a character of input already validated as UTF-8, and valid UTF-8 never encodes a surrogate or anything above U+10FFFF. |

The vt differential tests feed random and malformed bytes against vte, and
`batched_printing_matches_per_character_printing` checks the narrow path
against per-character printing, so a wrong proof shows up as a mismatch.

### Moving rows

`term/grid.rs` `rotate_left_one` rotates the row headers by one on every
line feed in a full-screen scroll region. `slice::rotate_left(1)` copies
through a buffer in three calls; this does one `ptr::read`, one
`ptr::copy` and one `ptr::write`.

- **Sound because** the first row is read out, the rest shift down inside
  the slice, and the first is written into the slot that frees up. Every
  row is owned exactly once again, and nothing in between can panic, so
  no row is dropped twice or leaked.
- **Called** only for non-empty slices.

### Operating-system calls

| Where | Call | Why it is sound |
|-------|------|-----------------|
| `dbus/mod.rs`, `notifier/freedesktop.rs` (2) | `libc::getuid` | Takes no arguments and cannot fail. Used for D-Bus `EXTERNAL` authentication and for a per-user icon directory, which is used only if it is ours and not group- or world-writable. |
| `term/iterm.rs` | `libc::gethostname` | Writes into a buffer that is writable for the length passed. The result is checked, and the name is cut at the first NUL. |
| `graphics/kitty/media.rs` | `shm_open`, `OwnedFd::from_raw_fd`, `shm_unlink` | The name is a valid C string. A non-negative result is a new descriptor that the `OwnedFd` then owns and closes. The object is unlinked straight away, as the kitty protocol requires, whatever happens next. |
| `graphics/kitty/media.rs` (macOS) | `mmap`, `munmap` | macOS cannot `read` shared memory. The mapping is fresh, read-only and `offset + size` bytes, within the size the descriptor reports; the bytes are copied out and the mapping is removed before returning. |
| `hugepages.rs` (Linux) | `madvise(MADV_HUGEPAGE)` | The range lies inside the buffer. The hint only changes how the kernel backs the pages, never their contents, and failures are ignored. |
| `gpu/vk/library.rs` (Linux, FreeBSD) | `dlopen`, `dlsym`, `dlclose`, `dlerror` | Names are C strings. A null handle or symbol is an error, never used. `dlerror`'s message is copied before any other `dl` call. The library is closed only when the `Entry` that owns it drops, after the instance made from it is destroyed. |
| `platform.rs` (macOS) | Objective-C `window`, then `setAlphaValue:` | `view` is the live `NSView` winit gave for the window; `window` returns its `NSWindow` or nil, checked before use, and both selectors take and return the types written. |

The size and range checks (`range()` in `media.rs`, the kitty size limits)
are safe code and are unit-tested. The calls themselves are covered where
CI can run them: shared-memory transfers on Linux and macOS.

### Vulkan

Our own bindings in `gpu/vk` (Linux and FreeBSD) call the driver through
function pointers the loader returns, which is `unsafe` by language rule.

| Where | What | Why it is sound |
|-------|------|-----------------|
| `entry.rs` | `vkGetInstanceProcAddr` from `dlsym`, and `cast` | The specification gives each function name one type; `cast` turns the loader's `PFN_vkVoidFunction` into the type named with it, and checks at compile time that both are function pointers of one size. |
| `entry.rs`, `instance.rs`, `surface.rs`, `swapchain.rs`, `device.rs`, `buffer.rs`, `image.rs`, `timeline.rs`, `commands.rs`, `recording.rs`, `shader.rs`, `pipeline.rs`, `descriptors.rs`, `draw.rs` | Calls into the driver | Every structure passed is `#[repr(C)]`, laid out as the specification declares it (tested field by field in `types_tests.rs`, `submit_types_tests.rs`, `pipeline_types_tests.rs`, `descriptor_types_tests.rs`, `draw_types_tests.rs`, `surface_types_tests.rs` and `swapchain_types_tests.rs`), and outlives the call; handles are live while their owner is; output pointers point to room for what the driver writes, which `call::enumerate` sizes from the count. Result codes become `Error`s, never panics. |
| `instance.rs` | `vkDestroyInstance` on drop | The instance owns the handle and is not `Clone`; each device holds an `Rc` of it, so it is destroyed only after its last device. |
| `surface.rs` | Making a surface from a window's handles, and `vkDestroySurfaceKHR` on drop | `Surface::new` is an `unsafe fn`: its caller vouches that the `wl_display` and `wl_surface`, or the Xlib display and window, stay live until the surface drops (a headless surface has none). The handles are non-null by type, and a surface is made only on an instance created with its platform's extensions, checked first. The surface holds an `Rc` of the instance, so it is destroyed before it. |
| `features.rs`, `instance.rs` | The feature chain | `Features` is boxed before its structures are linked, so the `pNext` pointers stay valid while it lives, and the create info points to it only during the call. |
| `swapchain.rs` | Making a swapchain and destroying it on drop | Made only on a device created with `VK_KHR_swapchain`, from the same instance as the surface, whose queue family the surface says it shows on; all three are checked first. The swapchain holds `Rc`s of its device and surface, so it is destroyed before both, and each view of its images holds an `Rc` of the swapchain, so it is destroyed after them. Remaking passes the old handle, which the driver retires even when that fails. |
| `device.rs` | `vkDestroyDevice` on drop | The device owns the handle and is not `Clone`; each buffer, image and timeline holds an `Rc` of it, so it is destroyed only after the last of them. |
| `buffer.rs`, `image.rs` | Building a buffer or image, and destroying and freeing it on drop | Handles are kept only after a call succeeds, so a failed call's output is never used; dropping a half-built buffer releases exactly what was made. Memory is mapped only when `choose_memory` picked a host-visible type, once. A swapchain's image is never destroyed or freed by `Image`: only its view is, before the swapchain it holds. |
| `timeline.rs` | Making a timeline, and `vkDestroySemaphore` on drop | The type info chained to the create info lives on the stack across the call; the device was made with `timelineSemaphore`. Commands hold an `Rc` of the timeline they raise and wait for the GPU before letting go, so it outlives the work that signals it. |
| `commands.rs`, `recording.rs` | Resetting, recording and submitting the command buffer, and `vkDestroyCommandPool` on drop | The pool is reset, and dropped, only after waiting on the timeline for the last submission, so its buffer is never pending then; a lost device has finished everything. Recording takes `&mut self`, so one buffer is recorded at a time, and the queue is used from one thread, as `Rc` keeps it there. Each submission raises the timeline to a value `Timeline::next` hands out, above every value before; a failed submission skips its value, which nothing waits for. The pool is made on the timeline's device, so the two always match. |
| `recording.rs` | Barriers and copies between buffers and images | A copy is recorded only after checking that its buffer and image are the commands' device's, made for the copy's direction, and hold the region (sums checked for overflow, the buffer offset a multiple of the texel size); both are then held in `Commands` until the next recording has waited for the GPU, or the commands are dropped, which waits too. Each copy is preceded by a barrier from the layout the image is in (the last one recorded for it, or the one the submissions so far left it in, kept in the image only once a submission succeeds) to the one the copy needs, after what used it before; a download is followed by a barrier that makes the buffer's bytes visible to the CPU. One recording should use an image at a time, as the layout is tracked per recording. |
| `shader.rs`, `pipeline.rs` | Making shader modules, the sampler, the layouts and the quad pipeline, and destroying them on drop | Code reaches the driver only as words `shader::words` read and checked (the SPIR-V magic, a whole number of words, a header, a version Vulkan 1.3 takes); the checked-in shaders are validated by `spirv-val` in CI. Each create call writes into a local, kept only once it succeeds, so a failed step's output is never used and dropping a half-built pipeline destroys exactly what was made, in reverse order: the pipeline, its layout, the set layout, then the sampler the set layout holds as immutable. The modules are dropped right after the pipeline is made from them, which the specification allows. Each object holds an `Rc` of its device. |
| `descriptors.rs` | Making the quad set's pool and allocating the set, and `vkDestroyDescriptorPool` on drop | The pool has room for the one set of the quad set layout, which the set is allocated with while the pipeline that owns the layout is held. Handles are kept only once a call succeeds; a failed allocation drops the pool. Destroying the pool frees the set; the set holds an `Rc` of the pipeline, and so of the device, which outlive it. |
| `draw.rs` | Updating the quad set, and recording rendering, binds, push constants and the draw | A draw is recorded only after checking that the set's pipeline, the quads, the atlas and the target are the commands' device's, that the quads were made for storage and hold every quad drawn, the atlas for sampling and the target for rendering, in the pipeline's format, of one layer, and not the atlas. The set is updated only when no recording or pending submission uses it: it draws once a recording, and not again until the timeline passes the last submission that drew with it, which `Recording::submit` records. Barriers move the atlas to be sampled and the target to be drawn to first; the set, quads, atlas and target are then held in `Commands` until the GPU is done, as for copies. |
| `buffer.rs` | `bytes` and `bytes_mut` | The mapping covers the buffer's size and lasts until the memory is freed on drop; `&mut self` keeps other views out. Copies may use the buffer: the caller reads it only once the timeline passed the submission that used it, and writes it only before recording a copy from it or after that submission (not checked). |

The layouts and the enumeration logic are unit-tested without a driver;
`loader_tests.rs` creates real instances, on Mesa's software driver in CI.

### Test-only

- `parser/dispatch_tests.rs`, `vt/tests.rs`, `render/painter.rs`
  (`both_variants`): call the v3 copies directly, after checking
  `has_v3()`, to compare them with the baseline.
- `gpu/vk/call_tests.rs`: fake drivers write items through the pointer
  `enumerate` passes, within the count it gave.
- `gpu/vk/driver_tests.rs`, `device_tests.rs`, `memory_driver_tests.rs`,
  `buffer_tests.rs`, `image_tests.rs`, `timeline_tests.rs`, `commands_tests.rs`, `recording_tests.rs`, `pipeline_driver_tests.rs`, `pipeline_tests.rs`, `descriptor_driver_tests.rs`, `descriptors_tests.rs`, `draw_driver_tests.rs`, `draw_rules_tests.rs`, `surface_driver_tests.rs`, `surface_query_tests.rs`, `swapchain_driver_tests.rs`, `swapchain_tests.rs`: the fake driver reads the structures and `pNext` chains the bindings
  pass, and writes through their output pointers, as a real driver would.
- `graphics/kitty/media.rs` tests (macOS): create, size and map a shared
  memory object the test owns, as a kitty client would.

## Adding unsafe code

1. Look for a safe way first: `std::simd`, a `std` API, or restructuring so
   that the compiler can elide the check. Measure it; keep `unsafe` only for
   a reproducible gain or when no safe API exists.
2. Put it in the smallest block, inside a safe function whose signature
   makes misuse impossible. If callers must ensure something, write an
   `unsafe fn` with a `# Safety` section instead of a safe function with a
   precondition.
3. Write the `// SAFETY:` comment to prove soundness, not to restate the
   code.
4. Test it differentially against the safe path (or the oracle crate), and
   run the tests in a debug build so overflow checks and debug assertions
   are on.
5. Add it to this file.
