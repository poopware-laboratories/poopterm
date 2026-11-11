# Unsafe code

Poopterm is written in safe Rust. `unsafe` is kept only for what is truly
unsafe: calls into C and the kernel that have no safe `std` equivalent, and
the call into a copy of a function compiled for CPU features checked at
runtime. It is never used for speed; when safe code is slower, the safe code
is optimized. This file lists every remaining block: what it does, why it is
sound, why it is not safe code yet, and what tests it. Update it in the same pull request that
adds, moves or removes an `unsafe` block.

## The rules

From [`AGENTS.md`](../AGENTS.md):

- Safe code first. `unsafe` only where safe Rust cannot express the
  operation at all: FFI and system calls without a safe `std` wrapper, and
  runtime CPU dispatch.
- Never `unsafe` for speed: optimize the safe version instead. Replacing
  `unsafe` with safe code is welcome even if it starts slightly slower.
- Keep it in the smallest block that needs it, behind a safe function.
- Every block holds one unsafe operation and starts with a `// SAFETY:`
  comment that says why that operation is sound.

The compiler enforces both: `Cargo.toml` denies clippy's
`undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block`, and
rustc's `unsafe_op_in_unsafe_fn`. An `unsafe fn` must still wrap each
unsafe operation inside it in its own commented block, and documents what
callers must ensure under `# Safety`. Three module trees still expect
several operations per block, each with its reason: `gpu::vk`,
`present::x11` and `present::wayland`, whose driver and library calls
get one safe wrapper each in the steps that follow.

### Where it is allowed

`Cargo.toml` denies `unsafe_code` everywhere else. The allow-list:

- **Module trees that are calls into C:** `gpu::vk` (Vulkan), `gpu::mtl`
  (Metal), `present::wayland` (libwayland-client), `present::x11` (libxcb)
  and `sys` (safe wrappers over the kernel calls the standard library
  lacks: PTY pairs, terminal settings, fork and exec among them). Each has one
  `#[allow(unsafe_code)]` on its `mod` line.
- **Files with operating-system or library calls:** `allocator`,
  `app/gpu`, `app/wayland`, `library`, `objc`, `about_menu` and
  `platform`. Each starts with `#![allow(unsafe_code)]` and a comment.
- **CPU dispatch:** `parser/print`, `render/painter` and `vt/utf8`, and in
  `mod.rs` files, where a file-wide allow would cover the whole tree, only
  the one function: `Cell::fill` and `vt::Parser::advance`.
- **Tests** that call C or a dispatched copy directly, under otherwise
  safe modules: `cgroup_tree_tests`, `fonts/data_tests`, `mapped_tests`,
  `parser/dispatch_tests`, `present/memory_tests`, `pty/reap_tests`,
  `pty/session_tests`, `shell_integration/tests`, `term/cell_tests`, `term/fill_tests`,
  `vt/tests` and `vt/utf8_tests`.

`scripts/pr_check.py` prints how many `unsafe` blocks, functions and impls
a pull request adds and removes, and fails one that adds more than it
removes, or adds to this list, without changing this file.

These have no `unsafe`: the escape-sequence state machine's logic, the
terminal model apart from the cell layout below, the font engine
(`src/fonts`), our inflate (`src/inflate`), the kitty and Sixel decoders,
input encoding and the UI. Dependencies have their own `unsafe` (winit,
softbuffer, mimalloc, `image`); replacing them (see
[`TODO.md`](../TODO.md)) moves that code here, under these rules.

## Why there is any at all

- **Runtime CPU dispatch.** A function compiled for AVX2 and the other
  x86-64-v3 features is safe to define, but calling it is `unsafe` unless
  the caller has the same features; each call sits right after the runtime
  check.
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
  first, in the same function. The copies themselves are safe
  `#[target_feature]` functions: only the call needs `unsafe`, and code
  compiled with the same features may call them without it.
- **Tested** by differential tests that run both copies on the same input
  and require identical results: `vt/tests.rs` (the tokenizer, also
  against vte), `parser/dispatch_tests.rs` (parsing and printing),
  `painter.rs` `both_variants` (glyph pixels), `term/fill_tests.rs` and
  `vt/utf8_tests.rs` (every variant against `std::str::from_utf8`).
- **Measured:** the v3 copies were kept only where they win (#125 for
  glyph blending); a v3 base64 decoder measured slower and was dropped
  (#36).

### Cell stores (no `unsafe`)

`term/fill.rs` fills runs of cells in safe code compiled with AVX2:
`as_chunks_mut::<2>()` and `fill([cell; 2])` become one 32-byte store per
pair, the instructions the intrinsics gave; only the call into that copy is
`unsafe`, after the runtime check. History rows are copied into the ring with
`copy_from_slice`. The non-temporal stores and the `sfence` that published
them are gone: Rust has no safe non-temporal store, and they existed only
for speed (log −19%, ls −17% without them; compact history rows in
`TODO.md` are the safe way back).

### Cells (no `unsafe`)

`Cell` is `#[repr(C, align(16))]`: `ch` (4 bytes), `fg` (4), `bg` (4),
`attrs` (1), `width` (1), `extra` (2), exactly 16 bytes with no padding. No
code reinterprets its memory any more. `Cell::bits` assembles the fields
with shifts, which compiles to one 16-byte load. Printed cells are stored
with `Cell::put` (the whole template cell in one 16-byte store, then the
character; `Cell { ch, ..template }` compiles to a store per field), runs
with `slice::fill`, and `trimmed_len` compares `bits`. A `const` assert next to `Cell`
keeps the layout these compile to.

### Validated text (no `unsafe`)

Printed text reaches `Handler::print` as a `vt::Text`: bytes the SIMD
validator (`vt/utf8.rs`) accepted, never turned into a `&str` with
`from_utf8_unchecked`. `vt::text::Chars` decodes them with slice patterns
(one length check covers a whole character away from the end) and a checked
`char::from_u32`, so a broken invariant would print U+FFFD, not read out of
bounds. `str::from_utf8` instead would validate every run twice; the first
safe version did, and cost CJK and emoji text over 20%. The tests decode
every code point and compare mixed text with `str::chars`; the vt
differential tests feed random and malformed bytes against vte.

### Operating-system calls

| Where | Call | Why it is sound |
|-------|------|-----------------|
| `sys/user.rs` `uid` | `libc::getuid` | Takes no arguments, touches no memory and cannot fail. Used (as `platform::uid`) for D-Bus `EXTERNAL` authentication, for a per-user icon directory (used only if it is ours and not group- or world-writable) and to look up the login shell. Tested against file ownership, `id -ru` and `/proc/self/status`. |
| `sys/user.rs` `hostname` | `libc::gethostname` | Writes into a 256-byte buffer, writable for the length passed. The result is checked, and the name is cut at the first NUL. `term/iterm.rs` caches it. Tested against `uname -n` and the kernel's. |
| `allocator.rs` (Linux) | `prctl(PR_SET_THP_DISABLE, 1)`, and `#[unsafe(link_section = ".init_array.00100")]` on the function pointer that calls it | Takes plain integers and sets one flag of this process; no memory is passed. The pointer is a plain `extern "C" fn()` the loader calls once, single-threaded, before `main` and before mimalloc's own constructor (priority 101), so nothing else runs yet. The flag is inherited through `fork` and kept across `execve`, so the shell's child clears it (`pty/spawn.rs`). Tested in `allocator_tests.rs` and `spawn_tests.rs`, each in a process of its own, and on the built binary in `tests/huge_pages.rs`. |
| `allocator.rs` | `mi_option_set` (and `mi_option_get` in tests), declared `safe` in an `unsafe extern` block | mimalloc's public C API, linked in by the `mimalloc` crate. Both take plain integers, accept any option index and are thread-safe, so no call can be unsound. The option's index comes from the vendored `mimalloc.h`; a test pins it by the option's default. |
| `library.rs` (Linux, FreeBSD) | `dlopen`, `dlsym`, `dlclose`, `dlerror` | Names are C strings. A null handle or symbol is an error, never used. `dlerror`'s message is copied before any other `dl` call. The library is closed only when its owner drops: the Vulkan `Entry`, after the instance made from it is destroyed, or the Wayland `Client`, after every proxy made through it. |
| `sys/pty.rs` | `posix_openpt`, `OwnedFd::from_raw_fd`, `grantpt`, `unlockpt`, `fcntl(F_SETFD)` (macOS) | Each takes flags or the master descriptor we just opened and own, and the results are checked. A non-negative `posix_openpt` result is a new descriptor that the `OwnedFd` owns from then on, so an early error closes it. macOS refuses `O_CLOEXEC` in `posix_openpt`, so the flag is set on the next line. The slave is opened with `OpenOptions` and `O_NOCTTY`, with no `unsafe`. Tested against portable-pty in `sys/pty_tests.rs`, and so is the closing on exec. |
| `sys/pty.rs` | `ptsname_r` (declared by us on macOS, where the libc crate lacks it) | Writes at most the buffer's length, NUL included, into a stack buffer, and the name ends at the first NUL. Never plain `ptsname`, whose one buffer per process other threads (portable-pty's `openpty` on FreeBSD, #250) overwrite. The macOS declaration matches libSystem's, present since macOS 10.13.4, below our minimum of 11. |
| `sys/pty.rs` | `ioctl(TIOCSWINSZ)`, `ioctl(TIOCGWINSZ)` | Each reads or writes exactly one `libc::winsize`, which lives on the stack through the call, on a descriptor borrowed through `AsFd`. A descriptor that is not a terminal fails with `ENOTTY` (tested). |
| `sys/termios.rs` | `tcgetattr` (with `MaybeUninit::assume_init`), `tcsetattr`, `cfsetispeed`, `cfsetospeed` | `tcgetattr` fills the whole `termios` on success, and only then is it read; on failure the error is returned and the memory dropped unread. `tcsetattr` reads one `termios` that outlives the call. The speed setters write a field of a `termios` we own, with a speed the platform defines. A descriptor that is not a terminal fails with `ENOTTY` (tested). |
| `sys/spawn.rs` | `fork`, then in the child `setsid`, `ioctl(TIOCSCTTY)`, ``dup2`, `close_range`/`syscall(SYS_close_range)` or `fcntl(F_SETFD)` per descriptor, `sigprocmask`, `sigaction`, `chdir`, `prctl(PR_SET_THP_DISABLE, 0)` (Linux), `execve`, `write`, `_exit` | The child of a threaded process may make only async-signal-safe calls, and every call after `fork` is one; none allocates, locks or unwinds. The path, `argv`, `envp` (NUL-terminated pointer arrays), working directory, empty signal set and default `sigaction` are all built before the fork and owned by a `Prepared` that outlives `execve`. `errno` goes back as four bytes on a pipe whose ends are close-on-exec, so a successful `execve` closes it and the parent reads nothing; on failure the child `_exit`s, running no destructors or `atexit` handlers. macOS's `TIOCSCTTY` (`_IO('t', 97)`, missing from the libc crate there) is copied from `<sys/ttycom.h>`. Tested from inside the program in `sys/spawn_tests.rs`: controlling terminal, session, signals, leaked descriptors, `exec` errors. |
| `sys/spawn.rs` | `sysconf`, `sigemptyset`, `mem::zeroed` for `sigset_t` and `sigaction` | In the parent: each writes only into a buffer or struct we own and checks its result. The pipe that reports `exec` errors is `std::io::pipe`, closed on exec, read safely. All zeroes is a valid `sigset_t` and `sigaction`, and zero in `sa_sigaction` is `SIG_DFL`. |
| `sys/process.rs` | `kill(pid, 0)`, `kill(pid, SIGHUP)`, `waitpid` (blocking, or with `WNOHANG`) | Each takes plain integers, and `waitpid` writes the status into an integer we own; results are checked, and a blocking wait retries on `EINTR`. IDs of 0 or below, which `kill` takes as process groups (0 is our own), are refused: `exists` says false and `hang_up` fails with `InvalidInput`. `sys/spawn.rs` hangs up and waits only for the child it started, and `sys/exits.rs` checks pids it was given (macOS, FreeBSD). Tested in `sys/process_tests.rs` and through `spawn_tests.rs`. |
| `platform.rs` | `msg_send![NSEvent, pressedMouseButtons]` (macOS) | A class method with no arguments that returns an `NSUInteger` bitmask of the buttons held now; it reads system state and changes nothing. Called on the main thread from the pointer-move handler, only while a selection drag is active. Tested through its pure use in `ui/drag_tests.rs`; the call itself is covered by running the app. |
| `sys/user.rs` `shell` | `getpwuid_r`, `CStr::from_ptr` | `getpwuid_r` writes only into a `passwd`, a buffer and a pointer we own, with the buffer's length passed; its result and the returned pointer are checked before anything is read. `pw_shell` is a NUL-terminated string inside that buffer, null is treated as none, and it is copied before the buffer drops. Never plain `getpwuid`, whose static buffer other threads share. Tested in `sys/user_tests.rs` against `/etc/passwd`, for unknown users and from many threads, and through `login_shell_tests.rs`. |
| `sys/exits.rs` | `epoll_create1`, `epoll_ctl`, `epoll_wait`, `syscall(SYS_pidfd_open)` (Linux); `kqueue`, `kevent`, `fcntl(F_SETFD)`, `mem::zeroed` for `kevent` (macOS, FreeBSD); `OwnedFd::from_raw_fd` | Each new descriptor (the epoll set or kqueue, each pidfd) is checked and owned by an `OwnedFd` at once, and closes on exec (`EPOLL_CLOEXEC`, pidfds by default, `FD_CLOEXEC` on the kqueue). `epoll_ctl` and `kevent` read one event we own; the waits write at most `BATCH` events into an array we own and retry on `EINTR`. All zeroes is a valid `kevent` (no data, null `udata`), and the fields set after are plain numbers. The stop pipe is a `std::io::pipe`, written safely. Tested in `sys/exits_tests.rs`: batches, double adds, concurrent waiters, `EINTR`, descriptor flags. |
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
| `stamps.rs`, `commands.rs`, `recording.rs`, `device.rs`, `instance.rs` | Timestamp queries | The five query functions are looked up together and used only if the driver has them all (`device.rs`). The pool of two queries belongs to one `Commands`, whose recording resets both and writes the first as it begins and writes the second just before the buffer ends; recording waits for the last submission first, so no query in flight is reset, and results are read only for a submission that went through, once finished, into two `u64`s eight bytes apart with `VK_QUERY_RESULT_64_BIT`. The pool is destroyed with the commands, after their wait. `Instance::timestamp_bits` enumerates the families as `call::enumerate` sizes them. `QueryPoolCreateInfo` is laid out as the specification declares it (tested in `stamps_tests.rs`). |
| `fault.rs`, `device.rs` | Device fault reports | `vkGetDeviceFaultInfoEXT` is looked up only for a device made with `VK_EXT_device_fault` and its `deviceFault` feature, and called only after a submission reported `VK_ERROR_DEVICE_LOST`, which the specification allows. The first call passes no info and writes only the counts; the second passes arrays of exactly as many items as the counts then say, each count first capped at eight, null where it is zero, no vendor binary (its feature is never enabled and its size is set to 0), and the driver writes no more than the counts. `Counts`, `AddressInfo`, `VendorInfo` and `Info` are laid out as the specification declares them (tested in `fault_tests.rs`). |
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

### Memory mappings and shared memory

`sys/mem.rs` is the one place that maps memory or opens shared memory
objects. `mapped.rs` (fonts), `present/memory.rs` (window buffers) and
`graphics/kitty/media.rs` (kitty `t=s` transfers) use it from safe code.

| Where | What | Why it is sound |
|-------|------|-----------------|
| `sys/mem.rs` `Map` | `mmap`, `munmap` on drop; slicing the mapping, and `Send`/`Sync` for it | Maps an open descriptor at an address the kernel picks, over no memory of ours; the result is checked against `MAP_FAILED`, and an empty mapping is refused as `mmap` refuses it. `bytes` covers exactly the mapped length and lives no longer than the `Map`, which unmaps it, with that length, only on drop. `bytes_mut` and `words_mut` exist only for mappings made writable and borrow `&mut self`, so they are its only view in this process; words cover the whole words only, and the page-aligned start aligns them. A `Map` owns its mapping as a `Box<[u8]>` owns its memory, and shared access only reads, so it may move and be shared between threads. Another process may still change or shrink what is mapped: fonts are parsed with bounds checks on every read, the kitty transfer is copied out at once, and the compositor or X server reads a window buffer only between a commit or put and its release. Reading past the end of an object shrunk in place faults (`SIGBUS`), as in any program that maps files. Tested in `sys/mem_tests.rs`, `mapped_tests.rs` (including `/proc/self/maps` for the permissions and the unmapping) and `present/memory_tests.rs`. |
| `sys/mem.rs` `anonymous`, `shm_open`, `shm_unlink` | `memfd_create` (Linux) or `shm_open(SHM_ANON)` (FreeBSD); `shm_open`, `shm_unlink`; `OwnedFd::from_raw_fd` | Names are C strings. A non-negative result is a new descriptor that an `OwnedFd` owns at once, closed on exec (`MFD_CLOEXEC`, `O_CLOEXEC`, and POSIX for named objects); a negative one is returned as the error. Unlinking only removes a name; open descriptors and mappings stay valid (tested). |


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
- `present/wayland/fake_tests.rs`, `fake_rules_tests.rs`: the fake
  libwayland's functions are `extern "C"`, called through the presenter's
  function pointers; they read requests' arguments by each request's
  signature, duplicate and `pread` the pool's descriptor as a compositor
  would, and call the dispatcher with the event's arguments.
- `present/wayland/real_tests.rs` (`on_real_surface`, which
  `app/wayland_tests.rs` uses through `outside_tests.rs`): connects to the
  compositor, binds `wl_compositor`, makes a surface for the presenter and
  destroys it before disconnecting.
- `present/memory_tests.rs`, `sys/mem_tests.rs`: `fcntl(F_GETFD)` on a
  descriptor the test owns, to check it closes on exec.
- `mapped_tests.rs`: `sysconf(_SC_PAGESIZE)`, `mkfifo` on a C string
  naming a path in the test's scratch directory, and `geteuid`.
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
