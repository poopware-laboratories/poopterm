# Security Policy

A terminal emulator reads bytes from every program you run, including
`cat` on a file you just downloaded, `curl` output, and SSH sessions to
machines you do not control. Poopterm treats all of that output as hostile,
so we take reports about it seriously.

## Supported versions

Poopterm ships rolling releases (`YEAR.MONTH.PATCH`, for example `26.10.10`).
Security fixes go into the next release on `main`; older releases are not
patched. Please check that a problem still exists in the
[latest release](https://github.com/poopware-laboratories/poopterm/releases/latest)
or on `main` before reporting it.

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report privately through GitHub:
[**Report a vulnerability**](https://github.com/poopware-laboratories/poopterm/security/advisories/new)
(Security tab → Advisories → Report a vulnerability).

Please include:

- the Poopterm version and your OS, display server and shell;
- the exact bytes or file that trigger the problem (a small reproducer such
  as a `printf` command is ideal), or the steps to reproduce it;
- what an attacker gains: code execution, reading or writing files, leaking
  data back through the terminal, a crash or hang, and so on.

What to expect:

- We aim to acknowledge your report within **7 days** and to keep
  you updated while we work on a fix.
- We fix the problem in a test-first commit, publish a release, and then
  publish the advisory, crediting you unless you ask us not to.
- Please give us a reasonable chance to release a fix before disclosing the
  problem publicly.

## Scope

In scope: anything a program running in the terminal can do through its
output, or a file Poopterm is asked to open. In particular:

- escape sequence parsing (CSI, OSC, DCS, APC) and the terminal state it
  changes, including crashes, hangs, and unbounded memory or CPU use;
- replies the terminal sends back to the program (queries must never leak
  data such as clipboard contents, files, or environment variables);
- image protocols: Sixel, the kitty graphics protocol (including its file,
  temporary file and shared memory transfers) and iTerm2 inline images, and
  the PNG, JPEG and WebP decoders behind them;
- clipboard access (OSC 52 and OSC 5522), hyperlinks (OSC 8) and opening
  links or files;
- notifications, window titles, and shell integration (OSC 7, OSC 133);
- the PTY layer and how shells are launched.

Out of scope:

- settings the user controls: environment variables (`$HOME`, XDG
  variables, `POOPTERM_*`), `config.toml`, and command-line arguments.
  Someone who can change these can already run code as you;
- attacks that require an attacker who already runs code as your user, or
  physical access to an unlocked machine;
- bugs in programs running inside the terminal, in your shell, or in the
  operating system;
- denial of service through ordinary heavy output that a user can stop by
  interrupting the program (report a hang or crash that survives that).

## How Poopterm defends itself

These are the rules every change follows (see [`AGENTS.md`](AGENTS.md));
breaking one of them is a vulnerability:

- explicit size limits on everything a program can make the terminal store
  (images, titles, hyperlinks, clipboard writes, scrollback);
- no answer to requests that would leak data: OSC 52 clipboard reads are
  ignored on purpose;
- unknown or unsupported sequences are ignored silently, never echoed back;
- kitty graphics file transfers read only absolute paths to regular files
  outside `/proc`, `/sys` and `/dev`, without blocking, within a size
  limit, and delete only specially marked files in temporary directories;
- links to executable or launchable files are copied to the clipboard
  instead of opened;
- parsers of untrusted input avoid `unsafe` unless it is proven necessary
  (every block is listed and justified in [`docs/unsafe.md`](docs/unsafe.md)),
  and replacements for third-party decoders are tested against the original
  crate with randomized and damaged input.
