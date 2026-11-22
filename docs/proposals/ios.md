# Proposal: Poopterm for iOS and iPadOS

A terminal for iPhone and iPad that builds on what a-Shell proved works,
and goes further: with JIT enabled through StikDebug, it runs real programs
at close to native speed instead of only built-in commands and interpreted
WebAssembly. This is a proposal; nothing of it is built yet. (Issue #150,
jailbroken iOS 12, is a separate target: there `fork`, `exec` and PTYs
exist, and Poopterm's macOS code mostly applies.)

## What iOS allows, and what a-Shell does with it

An iOS app is one sandboxed process. It cannot `fork`, `exec` or
`posix_spawn` other programs, has no PTYs, and may only run code Apple
signed, so it cannot map memory writable and executable.

a-Shell works inside those rules:

- **`ios_system`**: command-line tools (`ls`, `curl`, `ssh`, `vim`,
  Python, Lua, TeX…) are compiled into the app as frameworks. A "process"
  is a thread with its own `stdin`, `stdout`, `stderr`, environment and
  working directory; `system()`, `popen` and `posix_spawn` are replaced so
  that pipelines run as threads. Global state the tools keep is made
  per-thread where it matters.
- **WebAssembly**: programs the user compiles (with the bundled clang for
  WASI) or downloads run in a WASI runtime. Without JIT it interprets
  them (wasm3), several times slower than native code.

It is on the App Store because no unsigned native code runs: every native
binary is in the app, and WebAssembly is interpreted.

## What StikDebug changes

On iOS 17.4 and later, StikDebug (formerly StikJIT) enables JIT for a
sideloaded app without a computer: it talks to the device's own debug
services over a loopback VPN, using a pairing file made once with a
computer, and attaches the debugger to the app. A process being debugged
(`CS_DEBUGGED`) may map pages writable and then executable, which is what a
JIT needs. Apps like UTM, emulators and LiveContainer use this today.

Limits to design for:

- **Sideloading only.** JIT apps are installed with AltStore, SideStore or
  a developer account and refreshed every 7 days with a free account; the
  App Store will not take them.
- **JIT is per launch.** StikDebug must attach each time the app starts;
  without it the app falls back to what a-Shell can do.
- **Device generations.** Newer devices and iOS versions (the TXM changes
  in iOS 26) need StikDebug's newer attach methods; the app must detect
  whether executable memory works rather than assume it does.

## The design

### What carries over from Poopterm

| Part | On iOS |
|---|---|
| `src/vt`, `src/parser`, `src/term` | Unchanged: the tokenizer, terminal state, scrollback, images. |
| `src/gpu/mtl` (Metal renderer) | Draws into a `CAMetalLayer` in a `UIView`, as on macOS. |
| `src/fonts`, `src/render`, `src/ui` | Unchanged; touch selection and a keyboard accessory row are new UI models. |
| `src/input` | Key encoding is reused; input comes from `UIKeyInput` and hardware keyboards (`UIPress`) instead of winit. |
| `src/pty` | Replaced by an in-process "terminal device" (below). |
| `src/app` | A UIKit app delegate and view controller in place of winit's event loop. |

Rust targets `aarch64-apple-ios` and `aarch64-apple-ios-sim`; the
Objective-C runtime bindings in `src/objc` cover UIKit as they cover
AppKit.

### A terminal device without PTYs

Each tab gets a pair of in-process pipes and a line discipline written in
Rust: echo, canonical line editing, `^C`/`^Z`/`^D` turned into signals or
end of file, window size and `SIGWINCH`, raw mode on request. Programs see
`isatty()` true and get `tcgetattr`/`tcsetattr`/`TIOCGWINSZ` through the
runtime's replacements. Poopterm's own termios defaults (`src/pty/termios.rs`)
become the line discipline's.

### Three ways to run programs

1. **Built-in commands (always).** a-Shell's model: tools compiled into the
   app as frameworks run on threads through an `ios_system`-style layer.
   A small core (a shell, coreutils, `ssh`, `curl`, `git`, an editor) works
   on any install, App Store included.
2. **WebAssembly (always; fast with JIT).** WASI programs run in a runtime
   that interprets them without JIT and compiles them to native code with
   JIT. With JIT, Python, compilers and anything built for WASI run at a
   large fraction of native speed. WASI threads and sockets extensions
   cover servers and parallel tools.
3. **Native Linux arm64 programs (JIT only).** The part that runs "actual
   programs": ELF binaries built for Linux on arm64, such as Alpine's
   packages, unmodified.
   - **Loader.** An in-process ELF loader maps the program and its
     libraries (musl or glibc) at addresses of its own; JIT permission is
     what lets these pages become executable.
   - **System calls.** iOS has no Linux system calls, so each `svc #0` is
     rewritten, as the code is first mapped, into a call to a Rust
     implementation of the Linux system call it names, run on the same
     thread: files and directories (through the app's sandbox and a
     root file system image), sockets, `mmap`, `futex`, signals,
     `clone` for threads, the terminal device above. Everything else
     runs as the program's own native instructions, with no emulation of
     the CPU, so it is close to native speed.
   - **Processes.** There is still one address space. A "process" is a
     set of threads with their own file descriptors, memory regions,
     signal state and identity. `execve` loads the new program in place;
     `posix_spawn`, and `fork` followed by `exec` (what shells and build
     tools do), become "start a new virtual process"; programs that `fork`
     and keep running without `exec` (some servers) get a copy of their
     memory regions mapped elsewhere only where the program allows it, and
     otherwise fail with `ENOSYS`, which most handle.
   - **Packages.** A package manager over Alpine's arm64 repositories
     fills a root image in the app's container.
   x86-64 binaries could later run through a JIT translator in the same
   framework (QEMU's user mode or a box64-like translator), much slower.

### Where it stops

- Programs that need a real kernel: `ptrace` debuggers, containers, raw
  network devices, kernel modules.
- `fork` without `exec` in programs that depend on separate address
  spaces.
- Background work: iOS suspends apps it does not show; long jobs need the
  app in front or iOS's limited background modes.
- Without JIT (App Store build, or StikDebug not attached), only built-in
  commands and interpreted WebAssembly.

## Risks

- **Distribution.** Sideloading and a pairing file are a hurdle for most
  users; the App Store build would ship only tiers 1 and 2 (interpreted).
- **iOS changes.** Apple has narrowed JIT twice (iOS 17, iOS 26); the app
  must keep working, at a-Shell's level, if JIT stops being possible.
- **Correctness of the Linux layer.** Each system call is new code that
  untrusted programs exercise; it needs the same fuzzing and limits as the
  escape-sequence parser.
- **Licenses.** a-Shell and `ios_system` are BSD-licensed and Poopterm
  writes its own code (clean room, per AGENTS.md); bundled tools keep their
  own licenses.

## Steps

1. Poopterm's terminal on iOS with a Metal view, touch and keyboard input,
   and the in-process terminal device running a few built-in commands.
2. The `ios_system`-style layer with a shell, coreutils and `ssh`.
3. WebAssembly: interpreted first, then compiled when JIT is present.
4. Detecting JIT (StikDebug attached, executable memory available).
5. The ELF loader and the system call layer, starting with static musl
   binaries (BusyBox), then dynamic ones and Alpine's package manager.
6. Fuzzing of the system call layer; sideloading builds through
   AltStore/SideStore.
