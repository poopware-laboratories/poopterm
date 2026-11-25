# Proposal: running macOS command-line programs on iOS

Today every program Poopterm runs on an iPhone is ported by hand: its
sources compiled with the command shim (`ios/commands/poopterm_cmd.h`),
which turns `exit`, `fork`, `exec`, `isatty` and the terminal calls into
calls to the app, then linked as a library (`ios/packages/README.md`).
That gave us about 140 commands, but each one is a port to maintain.

The idea: run programs **built for macOS, unchanged**, such as Homebrew's
arm64 bottles, the way the loader already runs our downloaded packages
under JIT. macOS and iOS share the XNU kernel, the arm64 Mach-O format and
most of libSystem, so a macOS command-line program mostly calls functions
an iPhone also has. The layer has to load the program and stand in for
the parts that differ.

## What is already there

- `src/jit/macho.rs` reads arm64 dylibs with chained fixups.
  `src/jit/loader.rs` maps them into JIT memory, binds their imports
  against libraries already in memory, runs their initializers, and
  restores their data before each run.
- The in-process shell runs one program at a time, with pipes, the
  terminal device, and `fork`/`exec`/`waitpid` emulated
  (`src/inproc`, `ios/commands/poopterm_cmd.c`).
- `pkg` installs checked archives (`src/pkg`).

## What differs, and what the layer does about it

| macOS program expects | On iOS | The layer |
|---|---|---|
| To be an executable (`MH_EXECUTE`, `LC_MAIN`) that dyld starts | Only libraries can be loaded, and the loader refuses executables | Load PIE executables like libraries: map the segments, bind, then call the `LC_MAIN` entry with `argc`/`argv`/`envp`/`apple` |
| `exit`, `fork`, `execve`, `posix_spawn`, `waitpid`, `isatty`, `ioctl`, `tcgetattr`, `signal` from libSystem | They end the app or fail in the sandbox | Swap them in **at bind time**: when the program imports `_exit`, bind it to the shim's `poopterm_exit`. That is what the shim's macros do at compile time today, applied to a binary instead |
| The macOS platform in `LC_BUILD_VERSION` | dyld refuses other platforms, but our loader can accept them | Accept macOS (platform 1) as well as iOS (2); refuse arm64e, which iOS apps cannot run |
| Thread-local variables (`__thread_vars`; Rust's std, most C++) | The loader refuses them today | Set up TLV descriptors ourselves: one `pthread_key` per image and a thunk like dyld's `tlv_get_addr` |
| C++ exceptions and Rust panics, which unwind through `__unwind_info` | libunwind finds only images dyld knows | Register each image's unwind sections (`__register_frame` for `__eh_frame`, or a find-sections hook) |
| `/usr/lib/libncurses`, `libedit`, `libxml2`, `libiconv`, and Homebrew's `/opt/homebrew/opt/*/lib` | Some are in the iOS shared cache, some are not | Map install names: to iOS's own copy when it has one, to our builds (ncurses) or to the library's bottle under the package prefix otherwise |
| `/etc`, `/usr/local`, `/tmp` and `$HOME` paths | The sandbox | A prefix under `~/Library/poopterm`, with `HOME` and `TMPDIR` set as the shell sets them now |
| Objective-C classes and Swift metadata | Only dyld registers an image with the runtimes | Out of scope at first: plain C, C++, Rust and Go command-line programs |

## How it would be built

1. **Executables.** Teach `macho.rs` `MH_EXECUTE`, `LC_MAIN`, the macOS
   platform and a list of imports to swap at bind time. The test: a small
   C program built by `clang -target arm64-apple-macos` (not through
   `cc.sh`) prints, exits with a status and reads a pipe, all in process.
2. **Bind-time shim.** Move the shim's behaviour behind one table of
   names (`_exit`, `_fork`, `_execve`, `_isatty`...). Ported commands keep
   their compile-time macros, and loaded binaries get the same functions
   from the table. Differential tests run the same source both ways and
   compare.
3. **Thread-locals, initializers and unwinding.** This is enough for C++
   and Rust binaries. The test: a Rust binary that panics and catches the
   panic, and a C++ one that throws.
4. **Libraries and prefixes.** Map install names to libraries, with
   `@rpath`/`@loader_path` resolved inside the package prefix.
5. **Homebrew bottles.** `pkg brew install NAME` reads the formula's JSON
   from formulae.brew.sh, downloads the arm64 bottle from GitHub's
   registry, checks its SHA-256, follows its runtime dependencies, and
   unpacks into the prefix. Start with bottles that need no Objective-C:
   ripgrep, fd, bat, htop (it will find no processes but itself), and so
   on.

Each step is useful by itself: step 1 alone runs programs a user compiles
for macOS on their Mac and copies over.

## Limits that stay

- **JIT is required**, as for downloaded packages: StikDebug, then `jit
  enable`. On devices where code signing goes through TXM, the JIT path
  still needs checking on hardware.
- **One program at a time**, and no real `fork`. Programs that keep a
  child alive beside them (servers with workers, `tmux`, job control)
  will not work.
- **Not the App Store.** Loading unsigned code is exactly what review
  forbids. This is for sideloaded builds, like the rest of the JIT path.
- **arm64 only.** Apple's own `/usr/bin` programs are arm64e and
  platform-signed. Homebrew's and the user's own builds are plain arm64.

## Risks

- libSystem functions that exist on iOS but behave differently (some
  `sysctl` names, `getpwnam` in the sandbox, `posix_spawn` refused).
  Every swapped or refused call needs a test, and the list will grow
  with each bottle tried.
- Programs that make system calls directly, without libSystem. Go has
  used libSystem on Darwin since 1.11, and C, Rust and Swift do too, but
  hand-written assembly can still do it.
- Bottles linked against macOS-only frameworks (CoreServices, Security's
  macOS-only calls). Those should fail to load with the missing symbol
  named, not crash.
