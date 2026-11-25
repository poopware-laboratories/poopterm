# Packaging programs for Poopterm on iOS

iOS lets an app run no other process, so every command in Poopterm is a
**command library**: a program's C sources, compiled with a small shim and
linked into a dynamic library that the in-process shell calls. This guide
shows how to port a program, build it, and either bundle it with the app or
publish it as a `pkg` package.

## How a program runs

- `ios/commands/poopterm_cmd.h` is included before every source file
  (`-include`). Its macros send what would end or inspect a process to the
  shim: `exit` and `err` end the command, not the app; `isatty`, `ioctl`,
  `tcgetattr` and `tcsetattr` answer for the terminal device; `fork`,
  `exec`, `posix_spawn` and `waitpid` run the child inside the app, one at a
  time; signals whose default would end the app end only the command.
- `ios/commands/poopterm_cmd.c` is linked into every library. It exports
  `poopterm_entry`, which the shell calls with the arguments, environment
  and descriptors. `main` is renamed to `poopterm_main` by the header.
- The program reads keys and prints to the screen as it runs, with real
  termios behavior (canonical lines, raw mode, `^C`), so editors and pagers
  work. A program that opens `/dev/tty` (as `less` does) needs
  `-DPOOPTERM_DEV_TTY`, which makes that open give the keyboard.

## Where a library goes

| Kind | Built by | Ends up in | Needs JIT on a phone |
|---|---|---|---|
| Bundled command | `ios/commands/build.sh` | `Poopterm.app/Frameworks/<name>.framework` | No: signed with the app |
| Base package | `ios/packages/build.sh` (listed there) | the IPA, by `ios/bundle-ipa.sh` | No: signed with the app |
| Downloaded package | `ios/packages/build.sh`, published | `~/Library/poopterm/packages/bin` | Yes: run `jit enable` |

Every IPA ships the bundled commands and all the packages listed in
`ios/packages/build.sh`. Packages a user installs later with `pkg install`
are not signed, so on an iPhone or iPad they need JIT from
[StikDebug](https://github.com/StikDebug/StikDebug): `jit enable` asks it
to attach, and Poopterm loads the library with its own loader
(`src/jit/`). The simulator and macOS run them without JIT.

To use it on a phone: launch Poopterm from StikDebug's app list (it lists
apps signed with `get-task-allow`), choose StikDebug's universal script for
it, and run `jit enable`. Without that script the request ends the app.
`jit loader on` sends every command, bundled ones included, through the
loader, which is how to check it on a device.

## Porting a program

1. **Get the source** into `target/ios-ports/<name>` (a release tarball or a
   tag). Downloaded build scripts are never run: no `./configure`.
2. **Write its configuration by hand.** Where the program expects a
   generated `config.h`, write one in `ios/packages/<name>/` with the values
   that are true on iOS (see `ios/packages/curl/curl_config.h` and
   `ios/packages/jq/jq_config.h`). Turn off what iOS cannot do (other
   processes, `/proc`, private frameworks).
3. **Write `ios/packages/<name>/build.sh`.** It takes the SDK and the output
   path and calls `ios/packages/cc.sh` with the sources and flags, then puts
   the license in `share/doc/<name>/`:

   ```sh
   #!/bin/sh
   # Builds tree (GPL-2.0-or-later) as a Poopterm command library.
   #   ios/packages/tree/build.sh <iphonesimulator|iphoneos|macosx> <output>
   set -eu
   here=$(cd "$(dirname "$0")" && pwd)
   root=$(cd "$here/../../.." && pwd)
   source="$root/target/ios-ports/tree"
   "$here/../cc.sh" "$1" "$2" "$source"/*.c
   license="$(dirname "$2")/../share/doc/tree"
   mkdir -p "$license"
   cp "$source/LICENSE" "$license/LICENSE"
   ```

   A port with a library of its own builds that first as a static library:
   an output ending in `.a` makes `cc.sh` archive the objects instead of
   linking (see `ios/packages/curl/build.sh`).
4. **List it** in the `packages` table of `ios/packages/build.sh`:
   `name|version|port|summary`.
5. **Build and try it on the Mac first**, where the in-process shell runs
   the same libraries:

   ```sh
   ios/packages/build.sh macosx <name>
   POOPTERM_IN_PROCESS=1 POOPTERM_COMMANDS=target/ios-packages/work-macosx/<name>/bin:target/ios-commands/macosx cargo run
   ```

   `POOPTERM_LOADER=1` loads it with Poopterm's own loader instead of dyld,
   as a phone does for downloaded packages.
6. **Build for the simulator and the phone** with `iphonesimulator` and
   `iphoneos`, then `ios/bundle-sim.sh` or `ios/bundle-ipa.sh`.

### Common fixes

| Symptom | Fix |
|---|---|
| A header is missing from the iOS SDK (`termcap.h`, `term.h`, `tzfile.h`, `sys/ioctl_compat.h`) | Add a small one to `ios/commands/compat-ios/` declaring only what is used |
| FreeBSD's libmd (`md5.h`, `sha256.h`) | Already in `ios/commands/compat/`, over CommonCrypto |
| The program names something `err` itself | Build with `-DPOOPTERM_NO_ERR_H` |
| `main` relies on C's implicit `return 0` | Build with `-DPOOPTERM_KEEP_MAIN -Wl,-alias,_main,_poopterm_main` |
| It reads keys from `/dev/tty` | Build with `-DPOOPTERM_DEV_TTY` |
| It needs a private API (`libproc.h`, `vproc.h`, OpenDirectory) | Leave that feature out, or the program, on iOS |
| It uses thread-local variables | dyld loads it; our loader refuses it, so it cannot be a downloaded package |
| It starts background work or a pager | Set its configuration through the environment, as the shell does for git (`GIT_CONFIG_COUNT`) |

### What cannot work

Anything that needs another process alive at the same time (a pager in a
pipe still works, since stages run one after another, but a server and a
client do not), `fork` without `exec`, setuid, raw sockets, kernel
extensions, and other apps' files: the sandbox forbids them on every
iPhone that is not jailbroken.

## Publishing packages

`pkg` reads its index from the `packages-index` branch of Poopterm's own
repository unless told otherwise (`src/pkg/source.rs`):

```
https://raw.githubusercontent.com/poopware-laboratories/poopterm/packages-index/<platform>-index
```

There is one index per platform: `ios`, `ios-simulator` and `macos`, and
for the Android port `android-aarch64` and `android-x86_64`. Releases in
the repository are immutable, so the archives of each upload go in a
release of their own (`packages-ios-20261124-221500`), and the index names
each archive by its URL there. Only the small index files change on the
branch. Packages listed in `downloads` in `ios/packages/build.sh` are
published. The others ship with every IPA. To publish the iPhone's:

```sh
ios/packages/build.sh iphoneos
ios/packages/release.sh iphoneos      # stages them, prints the commands
```

Run the `gh release` commands it prints (create a draft, upload, then
publish), then commit `ios-index` to the `packages-index` branch. A phone
sees the new packages only once the index is pushed, after every archive
it names is there.

To use another source, or go back:

```sh
pkg source https://example.github.io/poopterm-packages/ios-index
pkg source default
```

`POOPTERM_PKG_SOURCE` in the environment overrides both. `pkg` checks
each archive's SHA-256 against the index before unpacking, and upgrades a
package when its checksum changes, even at the same version. Keep the
downloaded packages' licenses in the archive (`share/doc/<name>/`).
