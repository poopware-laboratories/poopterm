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
softbuffer, mimalloc, `image`); replacing them (see
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
| `parser/ascii.rs` `write` | Calls `Cell::words_mut` to store byte-derived characters and the template fields as whole cells. |
| `term/mod.rs` `Cell::fill` (portable path) | `&mut [Cell]` as `&mut [u64x2]`. |
| `term/scrollback.rs` `trimmed_len` | `&[Cell]` as `&[u64]`, to find trailing blank cells four at a time. |

The read-only views are always sound: any bit pattern is a valid integer.
The mutable views can write any bits, but a `char` has invalid values
(surrogates, above U+10FFFF), so writers must keep lane 0 a valid `char`:

- `Cell::fill` writes copies of a real `Cell`, so this holds.
- `Cell::words_mut` is an `unsafe fn`: its `# Safety` section makes each
  caller keep lane 0 a valid `char`. `parser/print.rs` stores `spacer`
  (a whole cell), `with_char` of a `char`, or characters decoded from
  validated UTF-8. `parser/ascii.rs` stores a byte converted to `u32` in
  lane 0 (always a scalar, at most U+00FF) and preserves all other template
  fields. Its safe wrapper checks equal slice lengths before any store;
  differential tests cover every byte, styles, subslices and length errors.

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
| `allocator.rs` | `mi_option_set` (and `mi_option_get` in tests), declared `safe` in an `unsafe extern` block | mimalloc's public C API, linked in by the `mimalloc` crate. Both take plain integers, accept any option index and are thread-safe, so no call can be unsound. The option's index comes from the vendored `mimalloc.h`; a test pins it by the option's default. |
| `hugepages.rs` (Linux) | `madvise(MADV_HUGEPAGE)` | The range lies inside the buffer. The hint only changes how the kernel backs the pages, never their contents, and failures are ignored. |
| `mapped.rs` | `mmap` (read only, private) and `munmap` on drop; slicing the mapping, and `Send`/`Sync` for it | Maps a regular, non-empty file, whole, at an address the kernel picks; the result is checked. The slice covers exactly the mapped length and lives no longer than the mapping, which is unmapped only on drop. Nothing in the process writes through it (`PROT_READ`), so sharing it between threads is like sharing a `&'static [u8]`. Another process could still change the file in place: package managers rename a new file over the old one, which leaves the mapping intact, but a font truncated in place while mapped would fault (`SIGBUS`) when its lost pages are read, as in any program that maps fonts. Fonts are parsed with bounds checks on every read, so changed contents cannot send a read out of bounds. Tested in `mapped_tests.rs`, including `/proc/self/maps` for the permissions and the unmapping. |
| `library.rs` (Linux, FreeBSD) | `dlopen`, `dlsym`, `dlclose`, `dlerror` | Names are C strings. A null handle or symbol is an error, never used. `dlerror`'s message is copied before any other `dl` call. The library is closed only when its owner drops: the Vulkan `Entry`, after the instance made from it is destroyed, or the Wayland `Client`, after every proxy made through it. |
| `pty/device.rs` | `posix_openpt`, `OwnedFd::from_raw_fd`, `grantpt`, `unlockpt`, `fcntl(F_SETFD)` (macOS) | Each takes flags or the master descriptor we just opened and own, and the results are checked. A non-negative `posix_openpt` result is a new descriptor that the `OwnedFd` owns from then on, so an early error closes it. macOS refuses `O_CLOEXEC` in `posix_openpt`, so the flag is set on the next line. The slave is opened with `OpenOptions` and `O_NOCTTY`, with no `unsafe`. Tested against portable-pty in `device_tests.rs`, and so is the closing on exec. |
| `pty/device.rs` | `ptsname_r` (declared by us on macOS, where the libc crate lacks it) | Writes at most the buffer's length, NUL included, into a stack buffer, and the name ends at the first NUL. Never plain `ptsname`, whose one buffer per process other threads (portable-pty's `openpty` on FreeBSD, #250) overwrite. The macOS declaration matches libSystem's, present since macOS 10.13.4, below our minimum of 11. |
| `pty/device.rs` | `ioctl(TIOCSWINSZ)`, `ioctl(TIOCGWINSZ)` | Each reads or writes exactly one `libc::winsize`, which lives on the stack through the call, on a descriptor borrowed through `AsFd`. A descriptor that is not a terminal fails with `ENOTTY` (tested). |
| `pty/termios.rs` | `tcgetattr` (with `MaybeUninit::assume_init`), `tcsetattr`, `cfsetispeed`, `cfsetospeed` | `tcgetattr` fills the whole `termios` on success, and only then is it read; on failure the error is returned and the memory dropped unread. `tcsetattr` reads one `termios` that outlives the call. The speed setters write a field of a `termios` we own, with a speed the platform defines. A descriptor that is not a terminal fails with `ENOTTY` (tested). |
| `pty/spawn.rs` | `fork`, then in the child `setsid`, `ioctl(TIOCSCTTY)`, ``dup2`, `close_range`/`syscall(SYS_close_range)` or `fcntl(F_SETFD)` per descriptor, `sigprocmask`, `sigaction`, `chdir`, `execve`, `write`, `_exit` | The child of a threaded process may make only async-signal-safe calls, and every call after `fork` is one; none allocates, locks or unwinds. The path, `argv`, `envp` (NUL-terminated pointer arrays), working directory, empty signal set and default `sigaction` are all built before the fork and owned by a `Prepared` that outlives `execve`. `errno` goes back as four bytes on a pipe whose ends are close-on-exec, so a successful `execve` closes it and the parent reads nothing; on failure the child `_exit`s, running no destructors or `atexit` handlers. macOS's `TIOCSCTTY` (`_IO('t', 97)`, missing from the libc crate there) is copied from `<sys/ttycom.h>`. Tested from inside the program in `spawn_tests.rs`: controlling terminal, session, signals, leaked descriptors, `exec` errors. |
| `pty/spawn.rs` | `pipe2`/`pipe` with `OwnedFd::from_raw_fd`, `fcntl(F_SETFD)` (macOS), `read`, `waitpid`, `kill`, `sysconf`, `sigemptyset`, `mem::zeroed` for `sigset_t` and `sigaction` | In the parent: each writes only into a buffer, integer or struct we own and checks its result; the two new pipe descriptors are owned right away. `kill` signals only the child we started. All zeroes is a valid `sigset_t` and `sigaction`, and zero in `sa_sigaction` is `SIG_DFL`. `try_wait` is `waitpid` with `WNOHANG` into an integer we own. |
| `platform.rs` | `msg_send![NSEvent, pressedMouseButtons]` (macOS) | A class method with no arguments that returns an `NSUInteger` bitmask of the buttons held now; it reads system state and changes nothing. Called on the main thread from the pointer-move handler, only while a selection drag is active. Tested through its pure use in `ui/drag_tests.rs`; the call itself is covered by running the app. |
| `pty/login_shell.rs` | `getpwuid_r`, `getuid`, `CStr::from_ptr` | `getpwuid_r` writes only into a `passwd`, a buffer and a pointer we own, with the buffer's length passed; its result and the returned pointer are checked before anything is read. `pw_shell` is a NUL-terminated string inside that buffer, null is treated as none, and it is copied before the buffer drops. Never plain `getpwuid`, whose static buffer other threads share. Tested in `login_shell_tests.rs`. |
| `pty/exits.rs` | `epoll_create1`, `epoll_ctl`, `epoll_wait`, `syscall(SYS_pidfd_open)` (Linux); `kqueue`, `kevent`, `fcntl(F_SETFD)`, `mem::zeroed` for `kevent`, `kill(pid, 0)` (macOS, FreeBSD); `write`, `OwnedFd::from_raw_fd` | Each new descriptor (the epoll set or kqueue, each pidfd) is checked and owned by an `OwnedFd` at once, and closes on exec (`EPOLL_CLOEXEC`, pidfds by default, `FD_CLOEXEC` on the kqueue). `epoll_ctl` and `kevent` read one event we own; the waits write at most `BATCH` events into an array we own and retry on `EINTR`. All zeroes is a valid `kevent` (no data, null `udata`), and the fields set after are plain numbers. `kill` with signal 0 sends nothing, only for pids above 0, which `kill` would take as process groups. The stop `write` sends one byte from a constant. Tested in `exits_tests.rs`: batches, double adds, concurrent waiters, `EINTR`, descriptor flags. |
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
| `entry.rs`, `instance.rs`, `surface.rs`, `swapchain.rs`, `present.rs`, `semaphore.rs`, `device.rs`, `allocation.rs`, `buffer.rs`, `image.rs`, `timeline.rs`, `commands.rs`, `recording.rs`, `shader.rs`, `pipeline.rs`, `descriptors.rs`, `draw.rs` | Calls into the driver | Every structure passed is `#[repr(C)]`, laid out as the specification declares it (tested field by field in `types_tests.rs`, `submit_types_tests.rs`, `pipeline_types_tests.rs`, `descriptor_types_tests.rs`, `draw_types_tests.rs`, `surface_types_tests.rs` and `swapchain_types_tests.rs`), and outlives the call; handles are live while their owner is; output pointers point to room for what the driver writes, which `call::enumerate` sizes from the count. Result codes become `Error`s, never panics. |
| `instance.rs` | `vkDestroyInstance` on drop | The instance owns the handle and is not `Clone`; each device holds an `Rc` of it, so it is destroyed only after its last device. |
| `surface.rs` | Making a surface from a window's handles, and `vkDestroySurfaceKHR` on drop | `Surface::new` is an `unsafe fn`: its caller vouches that the `wl_display` and `wl_surface`, or the Xlib display and window, stay live until the surface drops (a headless surface has none). The handles are non-null by type, and a surface is made only on an instance created with its platform's extensions, checked first. The surface holds an `Rc` of the instance, so it is destroyed before it. |
| `window.rs` | `GpuWindow::new` making the window's surface | `GpuWindow::new` is an `unsafe fn` with `Surface::new`'s contract: its caller vouches that the window's handles stay live until the `GpuWindow` drops. It holds the surface's only long-lived `Rc` with its swapchain, so the surface goes when the window does. |
| `app/gpu.rs` | `Gpu::open` opening a `GpuWindow` on the main window's handles | `Gpu::open` is an `unsafe fn` with `GpuWindow::new`'s contract: its caller vouches that the handles stay valid while the window it hands over lives. `Gpu::new` passes the winit window's own handles with a clone of its `Arc`, and `Gpu` declares the `GpuWindow` before the window, so the swapchain and surface drop first. |
| `present.rs` (`Recording::present`) | The present regions chain | `VkPresentRegionsKHR`, its one region and the rectangles live on the stack across `vkQueuePresentKHR`, laid out as the specification declares them (tested in `present_regions_tests.rs`); they are chained only on a device made with `VK_KHR_incremental_present`, and rectangles are cut to the image. |
| `features.rs`, `instance.rs` | The feature chain | `Features` is boxed before its structures are linked, so the `pNext` pointers stay valid while it lives, and the create info points to it only during the call. The one-flag structures of optional extensions sit in a `Vec` that is filled before linking and never grown, so its items do not move; `Instance::has_feature` chains only the structure of an extension the device offers. |
| `instance.rs` (`Instance::heap_free`) | The memory budget chain | `vkGetPhysicalDeviceMemoryProperties2` is loaded as optional, and asked only for a device made with `VK_EXT_memory_budget`; both structures live on the stack for the whole call, `MemoryBudget` is laid out as the specification declares it (tested in `memory_budget_tests.rs`), and a heap index past the sixteen it holds reads as unknown. |
| `instance.rs` (`Instance::driver`) | The driver properties chain | `vkGetPhysicalDeviceProperties2` is loaded as optional, so a loader without it leaves the driver unknown. Both structures live on the stack for the whole call, `DriverProperties` is laid out as the specification declares it (tested in `driver_info_tests.rs`), and it is the only structure chained. |
| `swapchain.rs` | Making a swapchain and destroying it on drop | Made only on a device created with `VK_KHR_swapchain`, from the same instance as the surface, whose queue family the surface says it shows on; all three are checked first. The swapchain holds `Rc`s of its device and surface, so it is destroyed before both, and each view of its images holds an `Rc` of the swapchain, so it is destroyed after them. Remaking passes the old handle, which the driver retires even when that fails. Dropping a swapchain first waits for its device's queue to go idle (a lost device has finished everything), so no submission or presentation still uses its images or semaphores when they go. |
| `present.rs` | Acquiring an image, and submitting a frame and presenting it | An image is acquired with a binary semaphore that is unsignalled with nothing pending: a fresh one, or one whose frame's submission the timeline has passed, which consumed its signal. A frame dropped before it is submitted leaves its semaphore signalled and never reuses it. The submission waits for the acquire semaphore at the stages its first barrier on the image waits for, moves the image to the present layout, and signals that image's drawn semaphore, which the presentation waits for; the image is acquired again only after that presentation, so its drawn semaphore is unsignalled again by then. The frame borrows the swapchain mutably, so it is presented with the swapchain it came from; commands of another device are refused. The present info and the arrays it points to are locals that outlive the call. |
| `semaphore.rs` | Making a binary semaphore, and `vkDestroySemaphore` on drop | The type info chained to the create info lives on the stack across the call. Semaphores are owned by the swapchain, which waits for the queue to go idle before they drop. |
| `device.rs` | `vkDestroyDevice` on drop | The device owns the handle and is not `Clone`; each buffer, image and timeline holds an `Rc` of it, so it is destroyed only after the last of them. |
| `buffer.rs`, `image.rs` | Building a buffer or image, and destroying and freeing it on drop | Handles are kept only after a call succeeds, so a failed call's output is never used; dropping a half-built buffer releases exactly what was made. Memory is mapped only when `choose_memory` picked a host-visible type, once. A swapchain's image is never destroyed or freed by `Image`: only its view is, before the swapchain it holds. |
| `timeline.rs` | Making a timeline, and `vkDestroySemaphore` on drop | The type info chained to the create info lives on the stack across the call; the device was made with `timelineSemaphore`. Commands hold an `Rc` of the timeline they raise and wait for the GPU before letting go, so it outlives the work that signals it. |
| `commands.rs`, `recording.rs` | Resetting, recording and submitting the command buffer, and `vkDestroyCommandPool` on drop | The pool is reset, and dropped, only after waiting on the timeline for the last submission, so its buffer is never pending then; a lost device has finished everything. Recording takes `&mut self`, so one buffer is recorded at a time, and the queue is used from one thread, as `Rc` keeps it there. Each submission raises the timeline to a value `Timeline::next` hands out, above every value before; a failed submission skips its value, which nothing waits for. The pool is made on the timeline's device, so the two always match. |
| `recording.rs` | Barriers and copies between buffers and images | A copy is recorded only after checking that its buffer and image are the commands' device's, made for the copy's direction, and hold the region (sums checked for overflow, the buffer offset a multiple of the texel size); both are then held in `Commands` until the next recording has waited for the GPU, or the commands are dropped, which waits too. Each copy is preceded by a barrier from the layout the image is in (the last one recorded for it, or the one the submissions so far left it in, kept in the image only once a submission succeeds) to the one the copy needs, after what used it before; a download is followed by a barrier that makes the buffer's bytes visible to the CPU. One recording should use an image at a time, as the layout is tracked per recording. |
| `shader.rs`, `pipeline.rs` | Making shader modules, the sampler, the layouts and the quad pipeline, and destroying them on drop | Code reaches the driver only as words `shader::words` read and checked (the SPIR-V magic, a whole number of words, a header, a version Vulkan 1.3 takes); the checked-in shaders are validated by `spirv-val` in CI. Each create call writes into a local, kept only once it succeeds, so a failed step's output is never used and dropping a half-built pipeline destroys exactly what was made, in reverse order: the pipeline, its layout, the set layout, then the sampler the set layouts hold as immutable. The pipeline layout takes two set layouts, the quad set's and the image texture's, both kept until the pipeline drops. The modules are dropped right after the pipeline is made from them, which the specification allows. Each object holds an `Rc` of its device. |
| `descriptors.rs` | Making the quad set's and each image texture's pool and allocating its set, writing a texture's set once when it is made, and `vkDestroyDescriptorPool` on drop | Each pool has room for the one set of its layout, which the set is allocated with while the pipeline that owns the layout is held. A texture's set is written before anything can record with it, pointing at an image of the same device that the set holds for as long as it lives. Handles are kept only once a call succeeds; a failed allocation drops the pool. Destroying the pool frees the set; the set holds an `Rc` of the pipeline, and so of the device, which outlive it. |
| `draw.rs` | Updating the quad set, and recording rendering, binds, push constants and a draw per run of quads with its image set bound | A draw is recorded only after checking that the set's pipeline, the quads, the atlas, every run's image set and the target are the commands' device's and pipeline's, that the runs lie within the quads drawn and no image is the target (a texture set is made only for an image made for sampling), that the quads were made for storage and hold every quad drawn, the atlas for sampling and the target for rendering, in the pipeline's format, of one layer, and not the atlas. The set is updated only when no recording or pending submission uses it: it draws once a recording, and not again until the timeline passes the last submission that drew with it, which `Recording::submit` records. Barriers move the atlas and each image to be sampled and the target to be drawn to first; the set, quads, atlas, image sets and target are then held in `Commands` until the GPU is done, as for copies. |
| `buffer.rs` | `bytes` and `bytes_mut` | The mapping covers the buffer's size and lasts until the memory is freed on drop; `&mut self` keeps other views out. Copies may use the buffer: the caller reads it only once the timeline passed the submission that used it, and writes it only before recording a copy from it or after that submission (not checked). |

The layouts and the enumeration logic are unit-tested without a driver;
`loader_tests.rs` creates real instances, on Mesa's software driver in CI.

### Objective-C

On macOS, `objc.rs` reaches the Objective-C runtime (libobjc, linked from
the system), through which the Metal renderer will reach Metal and Core
Animation. Sending a message is `unsafe` by language rule: the runtime
cannot check the method's types.

| Where | What | Why it is sound |
|-------|------|-----------------|
| `objc.rs` | `objc_getClass`, `sel_registerName`, `objc_retain`, `objc_release`, the autorelease pool push and pop | Names are C strings; a missing class comes back as nil and becomes `Missing`, never a message to nil. `Object` owns exactly one reference: `owned` takes over one a method returned to the caller (`alloc`, `new`, `copy`, `Create`), `retained` adds one for any other, `Clone` retains and `Drop` releases, so each reference is released once (`objc_tests.rs` checks the counts, `objc_probe_tests.rs` that an object is deallocated exactly when its last reference goes). A `Pool` is not `Send`, and pools pop in reverse order of their pushes, as guards on one thread do. |
| `objc.rs` | `objc_msgSend` cast to each method's type, in `Arguments::send` | The trampoline jumps to the method's implementation with the registers and stack as the caller set them, so calling it as the method's own C function type is calling that method; `transmute` turns one function pointer into another of the same size. Each call site's `SAFETY` comment names the argument and return types it passes, as Apple's documentation gives them, and no method returning a structure is sent (that would need `objc_msgSend_stret` on x86-64). `objc_probe_tests.rs` checks every arity we send with integers of each width, floats, pointers and structures by value. Selectors that may be missing are checked with `respondsToSelector:` first, as a missing one raises an exception that must not unwind into Rust. |
| `objc.rs` | `string`, `text`, `describe` | `initWithBytes:length:encoding:` copies the UTF-8 bytes before it returns, and consumes the `alloc`ed string it is sent to. `UTF8String` returns a C string, or null (read as empty), that lives as long as the string and the innermost pool, copied at once. |
| `objc.rs` | `objc_allocateClassPair`, `class_addMethod`, `objc_registerClassPair`, in `action_class` | Methods are added only to the class just allocated, before it is registered; a taken name comes back as nil and the registered class is looked up instead, never modified. The implementation is an `Action`, an `extern "C"` function taking the receiver, selector and sender and returning nothing, which is what the type encoding `v@:@` declares and how AppKit sends menu items' actions (`objc_tests.rs` sends one). |
| `about_menu.rs` (macOS) | `redirect`, `show_about_with` | `numberOfItems`, `itemAtIndex:` (below the count), `action`, `submenu`, `setTarget:` and `setAction:` are sent only to an `NSMenu` and its `NSMenuItem`s, as Apple documents them; `sharedApplication` and `mainMenu` run on the main thread, once the app has launched. The target an item is pointed at is never released, as items do not retain their targets. Our action is an `extern "C"` function that only runs a Rust callback behind a mutex and does not unwind. `about_menu_tests.rs` builds real menus and chooses the item. |

### Metal

`gpu/mtl` (macOS) reaches Metal through `objc.rs`, linking the Metal and
Core Graphics frameworks from the system.

| Where | What | Why it is sound |
|-------|------|-----------------|
| `gpu/mtl/device.rs` | `MTLCreateSystemDefaultDevice`, the command queue and the name | The function returns a device the caller owns, or nil (no Metal device), which becomes `NoDevice`. `newCommandQueue` returns a queue the caller owns; `name` an autoreleased string, retained and copied inside a pool, so it is released before `with` returns (`device_tests.rs` checks both on a fake device, and that a device refused its queue or name is released once). Core Graphics is linked, as Metal needs for the default device. |
| `gpu/mtl/buffer.rs` | Making a buffer, and `bytes_mut` | `newBufferWithLength:options:` returns a buffer the caller owns, or nil. A shared buffer's `contents` points to its `length` bytes for as long as it lives; `&mut self` makes `bytes_mut` the only view in this process, and the renderer writes it only after waiting for the last frame, the only one that reads it. |
| `gpu/mtl/texture.rs` | Making a texture, and `replaceRegion:…` | Sizes of no texels, and flat textures of several layers, are refused before Metal sees them. The descriptor is owned and released once the texture is made, or refused. Every upload's region is checked to lie inside the texture, its sums checked for overflow (`texture_tests.rs` checks this against a brute-force oracle), and to be filled by the texels passed, and its slice to be one the texture has; `MTLRegion` is `#[repr(C)]` as the header lays it out. The copy is made before the call returns. |
| `gpu/mtl/window.rs` | Making the `CAMetalLayer`, handing it to the window's view, `nextDrawable`, and the drawable's size and scale | `MetalWindow::new` is an `unsafe fn`: its caller vouches that the `NSView` outlives the window and that both are used only from the main thread. The view is given the layer before it is told it wants one, so it hosts ours (`window_tests.rs` checks the order on a fake view, and that no reference to the view is kept). `nextDrawable` returns an autoreleased drawable or nil (a frame drawn again), retained inside a pool; its texture is a 2D BGRA texture to draw to, taken at the size it reports (`Texture::made`), and the drawable is presented by the command buffer that drew it, which holds it until shown. `CGSize` is `#[repr(C)]` as two `f64`s. |
| `app/gpu.rs` (macOS) | `Gpu::open` opening a `MetalWindow` on the window's `NSView` | `Gpu::open` is an `unsafe fn` with `MetalWindow::new`'s contract. `Gpu::new` passes the winit window's own view, from the main thread, which alone draws, with a clone of the window's `Arc`; `Gpu` declares the `MetalWindow` before the window, so the layer goes first. |
| `gpu/mtl/pipeline.rs` | Compiling the MSL, making the functions and the pipeline | The source is our own `shaders/quad.metal`. Each `new…` returns an object the caller owns, or nil with an autoreleased `NSError` that a pool holds until it is described; both become errors (`pipeline_tests.rs` checks each refusal on the fake device, and that the error is released). The descriptor holds the functions it is given and its color attachment until the state is made from it. |
| `gpu/mtl/commands.rs` | Command buffers, the render encoder, draws, commit and wait | A draw is encoded only after `drawable` checks that its runs lie within the quads (so every instance the shaders read is in the buffer), that the target was made to be drawn to, has one layer, and is neither the atlas nor an image it samples. The command buffer and encoder are retained only while used, inside a pool, and the encoder always ends. The size passed as bytes is two floats on the stack, copied by the call. Command buffers retain what their commands use until the GPU is done. A failed frame's error is read after waiting. `commands_tests.rs` checks every call a draw encodes, in order, on the fake. |

### Wayland

`present/wayland` (Linux and FreeBSD) shows software frames through
`wl_shm`, calling libwayland-client through function pointers from
`dlsym`, in shared memory from `present/memory.rs` (below).

| Where | What | Why it is sound |
|-------|------|-----------------|
| `client.rs` | `cast` of each `dlsym` result | Each name is a function wayland-client-core.h declares with the type of the field it goes to, or an exported `wl_interface`, only ever passed back by address; `cast` checks at compile time that the target is pointer-sized. Requests use `wl_proxy_marshal_array_flags` and events `wl_proxy_add_dispatcher`, so no variadic call crosses the boundary. |
| `presenter.rs` | Requests, dispatching and destroying proxies | `Presenter::new` is an `unsafe fn`: its caller vouches that the `wl_display` and `wl_surface` outlive the presenter and are used from its thread. Every request passes a `wl_argument` array matching its signature in wayland.xml, and a new proxy's version as libwayland's own wrappers would. Our proxies live on our own queue, made through a display wrapper (destroyed right after the registry is made), so only `present` and `new` dispatch their events. The events go to a boxed `Events` whose address stays put; it is freed only in `Drop`, after every proxy on the queue is destroyed (buffers and pools with their destructor requests, `wl_shm` with `release` from version 2) and then the queue. A buffer destroyed while the compositor holds it is allowed by the protocol, which keeps what it shows. |
| `app/wayland.rs` | `open_for` and `open`, making a presenter for a window's `wl_display` and `wl_surface` | `open` is an `unsafe fn` with `Presenter::new`'s contract. `open_for` passes the winit window's own handles, from the UI thread, which alone draws; the window's `Software` holds the presenter in its sink, declared before a clone of the window's `Arc`, so the presenter goes before the window and its connection. |

The pool and the rows each frame copies are pure and unit-tested;
`fake_tests.rs` is a fake libwayland and compositor that checks each request
against the protocol's rules, and `real_tests.rs` runs the presenter against
a real compositor (headless weston in CI). `x11/fake_tests.rs` is a fake
libxcb and X server that serves requests when told to, reads shared memory
only then, and checks each request; `x11/real_tests.rs` reads frames back
from a real server (Xvfb in CI).

### X11

`present/x11` (Linux and FreeBSD) shows software frames on an X11 window
through libxcb, opened at run time, on the connection under winit's Xlib
display: in memory shared with the server (MIT-SHM 1.2, by descriptor), or
in `PutImage` requests.

| Where | What | Why it is sound |
|-------|------|-----------------|
| `client.rs` | `cast` of each `dlsym` result | Each name is a function xcb.h, xproto.h, shm.h or Xlib-xcb.h declares with the type of the field it goes to, or `xcb_shm_id`, only ever passed back by address; `cast` checks at compile time that the target is pointer-sized. Reply and cookie structures are `#[repr(C)]` as xproto.h and shm.h lay them out. |
| `presenter.rs` | Requests, replies and the setup | `Presenter::new` is an `unsafe fn`: its caller vouches that the Xlib display and window outlive the presenter and are used from its thread. The setup is read only as long as its own length field says. Each request passes what its xproto.h declaration takes: a `PutImage` holds exactly its rows' pixels and fits the server's maximum request length (`put_rows`); a `ShmPutImage` names a segment the server attached, holding the whole frame. Replies and errors come from `malloc` and are freed once read. A buffer is written only once the round trip sent after its last put is answered, as the server has read it by then; the descriptor given to `AttachFd` is a duplicate, which libxcb closes. `Drop` detaches every segment, discarding unanswered round trips, and frees the GC. |

### Shared memory

| Where | What | Why it is sound |
|-------|------|-----------------|
| `present/memory.rs` | `memfd_create` (Linux) or `shm_open(SHM_ANON)` (FreeBSD), `ftruncate`, `mmap`, `munmap` | The descriptor is new and owned by an `OwnedFd`, closed on exec. The file is sized before it is mapped, the size is at most `i32::MAX` bytes (what `wl_shm.create_pool` can say) and checked for overflow, and the shared mapping is checked against `MAP_FAILED` and unmapped with its length on drop. `pixels` borrows `&mut self`, so it is the mapping's only view in this process; the compositor or X server reads it only between a commit or put and its release, and the pool never draws into a held buffer. |


### Test-only

- `parser/dispatch_tests.rs`, `vt/tests.rs`, `render/painter.rs`
  (`both_variants`): call the v3 copies directly, after checking
  `has_v3()`, to compare them with the baseline.
- `gpu/vk/call_tests.rs`: fake drivers write items through the pointer
  `enumerate` passes, within the count it gave.
- `gpu/vk/driver_tests.rs`, `device_tests.rs`, `memory_driver_tests.rs`,
  `buffer_tests.rs`, `image_tests.rs`, `timeline_tests.rs`, `commands_tests.rs`, `recording_tests.rs`, `pipeline_driver_tests.rs`, `pipeline_tests.rs`, `descriptor_driver_tests.rs`, `descriptors_tests.rs`, `draw_driver_tests.rs`, `draw_rules_tests.rs`, `surface_driver_tests.rs`, `surface_query_tests.rs`, `swapchain_driver_tests.rs`, `swapchain_tests.rs`, `present_driver_tests.rs`, `present_tests.rs`: the fake driver reads the structures and `pNext` chains the bindings
  pass, and writes through their output pointers, as a real driver would.
- `gpu/vk/window_tests.rs`, `window_open_tests.rs`, `window_gpu_tests.rs`,
  `outside_tests.rs`, and `app/gpu_open_tests.rs` (through `Gpu::open`):
  open a `GpuWindow` on the fake driver, which only records the window's
  handles, or on a headless surface, which has none.
- `objc_probe_tests.rs` (macOS): registers classes of its own with the
  runtime, once each, whose methods are `extern "C"` functions taking the
  receiver and selector first, as the runtime calls them; weak references
  live in boxes, initialized and destroyed once each.
- `gpu/mtl/window_tests.rs` (macOS): a fake `NSView` class registered
  the same way records the layer it is given; `CATransaction flush`
  commits the thread's implicit transaction, as a run loop would.
- `gpu/mtl/fake_tests.rs`, `fake_commands_tests.rs` (macOS): a fake
  `MTLDevice`, queue, command buffer, encoder, library, buffer and texture,
  classes registered as `objc_probe_tests.rs` registers its own, whose
  methods hand over objects they own, or autorelease what they return
  unowned, as Metal's do; they read the descriptors, texels and bytes they
  are passed within the sizes passed with them, and a fake buffer's bytes
  are an `NSMutableData` it holds as an associated object;
  `buffer_tests.rs` and `texture_tests.rs` read a buffer's `contents` and
  a texture's texels (`getBytes:…`) within their sizes.
- `graphics/kitty/media.rs` tests (macOS): create, size and map a shared
  memory object the test owns, as a kitty client would.
- `present/wayland/fake_tests.rs`, `fake_rules_tests.rs`: the fake
  libwayland's functions are `extern "C"`, called through the presenter's
  function pointers; they read requests' arguments by each request's
  signature, duplicate and `pread` the pool's descriptor as a compositor
  would, and call the dispatcher with the event's arguments.
- `present/wayland/real_tests.rs` (`on_real_surface`, which
  `app/wayland_tests.rs` uses through `outside_tests.rs`): connects to the
  compositor, binds `wl_compositor`, makes a surface for the presenter and
  destroys it before disconnecting.
- `present/memory_tests.rs`: `fstat`, `pread` and `fcntl` on the
  memory's own descriptor.
- `present/x11/fake_tests.rs`: the fake libxcb's functions are
  `extern "C"`, called through the presenter's function pointers; they read
  `PutImage` pixels by the length given, take ownership of the descriptor
  `AttachFd` sends as libxcb does, and return replies from `malloc`.
- `present/x11/real_tests.rs`: opens a display, makes and maps a window of
  the root's depth, and reads it back with `GetImage`, freeing each reply.

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
