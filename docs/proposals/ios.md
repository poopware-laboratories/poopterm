# Proposal: Poopterm for iOS and iPadOS

A terminal for iPhone and iPad that builds on what a-Shell proved works,
and goes further: with JIT enabled through StikDebug, it runs real programs
at close to native speed instead of only built-in commands and interpreted
WebAssembly. Parts of it are built on the local `local/ios` branch (the terminal, the in-process shell and Apple's commands, in the simulator). (Issue #150,
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

### Running programs: a shell, not Linux

The app is a shell, as a-Shell is, and every program it runs is a native
iOS program: no Linux binaries, no system call translation.

1. **Commands in the app (always).** The in-process shell (`src/inproc`)
   has a line discipline, words, quotes, variables, `;`, `&&`, pipelines
   and redirections, and keeps as built-ins only what a shell must
   (`cd`, `export`, `exit`…). Every other command is a real program:
   `uname`, `ls`, `cat`, `sort`… are built from Apple's own sources
   (shell_cmds, file_cmds, text_cmds) as libraries by
   `ios/commands/build.sh`, bundled in `Poopterm.app/commands` and found on
   `$PATH`. Bundled with the app, they are signed with it, so they run
   without JIT. A pipeline runs its stages one after another, each reading
   what the one before printed.
2. **Packages (with JIT).** A command is a dynamic library built for iOS
   arm64 from the program's own source, its `main` renamed to an entry the
   shell calls with `argc`/`argv`, built with a small compatibility header
   (`exit` ends the command, not the app; `fork`/`exec` go through the
   shell). Darwin programs (fastfetch, htop, jq, ripgrep, git…) port with
   little change, since iOS is Darwin.
   - **`pkg`** (`src/pkg`), a shell built-in: `pkg source URL` sets
     where packages come from (`POOPTERM_PKG_SOURCE` overrides it), and
     `update`, `list`, `search`, `install`, `upgrade` and `remove` work
     on an index of one line per package (name, version, SHA-256 and
     size of its archive, archive path, summary). An archive is a
     `tar.gz` of command libraries in `bin/`, built by
     `ios/packages/build.sh` the way the bundled commands are. `pkg`
     fetches over HTTPS only (or `file://`, or this machine for testing)
     through Foundation's URL loading, checks size and SHA-256 before
     unpacking with its own gzip and tar readers (no links, nothing
     outside the package), unpacks beside the old version and swaps it
     in, and links the commands into
     `~/Library/poopterm/packages/bin`, which is on `$PATH`. Still to
     come: an Ed25519 signature on the index, and hosting for it.
   - **Loading:** iOS loads only code signed by Apple or the developer.
     While StikDebug has the app attached (`CS_DEBUGGED`), unsigned
     libraries load; without it, `pkg` says JIT is needed and the
     built-ins still work. The simulator does not enforce signing, which
     is how packages are developed and tested.
   - **Input and output:** while a command runs, its standard input,
     output and error are the tab's terminal device. Commands run one at a
     time per app, since file descriptors are shared by the process.
3. **WebAssembly (later).** WASI programs for what is not ported yet,
   interpreted without JIT and compiled with it.

### Where it stops

- Programs that need `fork` without `exec`, `ptrace`, or other processes.
- Background work: iOS suspends apps it does not show.
- Without JIT (App Store build, or StikDebug not attached), only the
  built-in commands (and later WebAssembly).

## Risks

- **Distribution.** Sideloading and a pairing file are a hurdle for most
  users; the App Store build would ship only tiers 1 and 2 (interpreted).
- **iOS changes.** Apple has narrowed JIT twice (iOS 17, iOS 26); the app
  must keep working, at a-Shell's level, if JIT stops being possible.
- **Package trust.** Packages run inside the app with its access; the index
  and every archive are signed, and `pkg` refuses what does not verify.
- **Licenses.** a-Shell and `ios_system` are BSD-licensed and Poopterm
  writes its own code (clean room, per AGENTS.md); bundled tools keep their
  own licenses.

## Steps

1. Poopterm's terminal on iOS with a Metal view, touch and keyboard input,
   and the in-process terminal device running a few built-in commands.
2. The `ios_system`-style layer with a shell, coreutils and `ssh`.
3. WebAssembly: interpreted first, then compiled when JIT is present.
4. Detecting JIT (StikDebug attached, executable memory available).
5. `pkg` and the package format; the first ports (fastfetch, jq,
   ripgrep) built as command libraries.
6. A scene delegate for iOS 27, keyboard avoidance, the Metal renderer on
   iOS; sideloading builds through AltStore/SideStore.
