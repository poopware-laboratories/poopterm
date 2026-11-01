#!/usr/bin/env python3
"""Terminal-agnostic output throughput benchmark over real-world workloads.

Measures how fast the terminal *running this script* consumes output, in the
spirit of kitty's `kitten __benchmark__`, but using nothing terminal-specific:

* Workloads come from poopterm's corpus (`poopterm --bench-corpus`): build
  logs, colored compiler output, ls, highlighted source, Unicode, CJK, emoji,
  full-screen TUI redraws, editor scrolling, progress bars, truecolor art,
  shell prompts with OSC 133, Sixel and kitty graphics. They are
  deterministic, so every terminal gets identical bytes. It uses the local
  release build, then `poopterm` from PATH. Set POOPTERM_BIN to choose a binary.
* Each measurement ends with a Primary Device Attributes query (`CSI c`),
  which every VT100-compatible terminal answers only after parsing all prior
  output, so buffering inside the terminal cannot inflate the numbers.
* One warm-up pass, then `--repeat` timed passes; the median is reported
  along with min/max so noisy runs are visible.
* Workloads are generated for the window's size, which is recorded, because
  rendering cost scales with the grid. Compare terminals at the same size.
* The terminal's resident memory is reported too; see docs/performance.md.

Usage (run inside the terminal under test):

    python3 scripts/bench-throughput.py                    # 16 MiB x 3 per workload
    python3 scripts/bench-throughput.py --mib 64 --repeat 5 --only log --only tui
    python3 scripts/bench-throughput.py --output results.json --format json
"""
import argparse
from collections import namedtuple
import json
import os
from pathlib import Path
import select
import shutil
import statistics
import subprocess
import sys
import termios
import time
import tty

CHUNK = 1 << 16  # bytes per write(2) call, identical for every terminal
ROOT = Path(__file__).resolve().parents[1]

# `data` is the timed payload; `warm_up` is about an eighth of it. Both end
# on whole bursts, so no escape sequence or UTF-8 character is cut: a DA1
# query sent after a cut sequence is swallowed by some terminals (kitty).
Payload = namedtuple("Payload", "key name data warm_up")


class NoReply(Exception):
    pass


def proc_stat(pid):
    """(parent, terminal) of `pid` from /proc, else from `ps`; None if gone."""
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
        fields = text[text.rindex(")") + 1:].split()
        return int(fields[1]), fields[4]
    except FileNotFoundError:
        if Path("/proc/self").exists():
            return None
    except (OSError, ValueError, IndexError):
        return None
    ps = subprocess.run(["ps", "-o", "ppid=,tty=", "-p", str(pid)], capture_output=True, text=True)
    fields = ps.stdout.split()
    return (int(fields[0]), fields[1]) if ps.returncode == 0 and len(fields) == 2 else None


def terminal_pid(pid, stat=proc_stat):
    """The first ancestor of `pid` not attached to the terminal `pid` is:
    the terminal emulator serving it. None if the chain ends first."""
    own = stat(pid)
    for _ in range(64):
        if own is None:
            return None
        info = stat(pid)
        if info is None:
            return None
        if info[1] != own[1]:
            return pid
        pid = info[0]
    return None


def parse_status(text):
    """Resident and peak resident KiB from /proc/<pid>/status."""
    fields = dict(line.split(":", 1) for line in text.splitlines() if ":" in line)
    kib = lambda name: int(fields[name].split()[0]) if name in fields else None
    return {"rss_kib": kib("VmRSS"), "peak_kib": kib("VmHWM")}


def memory(pid):
    """What `pid` holds resident now and at its peak, in KiB; None if unknown."""
    try:
        return parse_status(Path(f"/proc/{pid}/status").read_text())
    except FileNotFoundError:
        if Path("/proc/self").exists():
            return None
    except OSError:
        return None
    ps = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
    rss = ps.stdout.strip()
    return {"rss_kib": int(rss), "peak_kib": None} if ps.returncode == 0 and rss.isdigit() else None


def mib(kib):
    return None if kib is None else round(kib / 1024, 1)


def wait_for_terminal(fd, timeout, query=b""):
    """Send `query` and DA1, and block until the DA1 reply (`ESC [ ? ... c`)
    arrives. Returns everything the terminal answered."""
    old = termios.tcgetattr(fd)
    tty.setraw(fd)
    try:
        os.write(sys.stdout.fileno(), query + b"\x1b[c")
        reply = b""
        deadline = time.monotonic() + timeout
        while not (b"\x1b[" in reply and reply.endswith(b"c")):
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
                raise NoReply("terminal did not answer the DA1 query (CSI c)")
            reply += os.read(fd, 256)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old)
    return reply


def parse_xtversion(reply):
    """The name in an XTVERSION reply (`DCS > | name ST`), if any."""
    # A linear scan: a regex search restarts at every opener, which is
    # quadratic on a reply full of openers and no terminator.
    start = reply.find(b"\x1bP>|")
    if start < 0:
        return None
    end = reply.find(b"\x1b\\", start + 4)
    return reply[start + 4:end].decode(errors="replace") if end >= 0 else None


def poopterm_binary():
    configured = os.environ.get("POOPTERM_BIN")
    binary = Path(configured).expanduser() if configured else ROOT / "target/release/poopterm"
    if not configured and not binary.is_file():
        installed = shutil.which("poopterm")
        if installed:
            binary = Path(installed)
    if not binary.is_file():
        raise FileNotFoundError(
            f"{binary} not found; run `cargo build --release`, install poopterm, or set POOPTERM_BIN"
        )
    return binary


def workloads(binary):
    """(name, description) of every corpus workload."""
    listing = subprocess.run([binary, "--benchmark", "--list"], capture_output=True, text=True, check=True)
    return [tuple(line.split(None, 1)) for line in listing.stdout.splitlines() if line.strip()]


def corpus(binary, name, kib, columns, lines):
    return subprocess.run([binary, "--bench-corpus", name, "--kib", str(kib), "--size", f"{columns}x{lines}"],
                          capture_output=True, check=True).stdout


def build_payloads(mib, columns=120, lines=40, only=None, binary=None):
    binary = binary or poopterm_binary()
    payloads = []
    for key, name in workloads(binary):
        if only and key not in only:
            continue
        data = corpus(binary, key, mib << 10, columns, lines)
        # A smaller build is a prefix of the larger one: same seed, whole bursts.
        warm_up = corpus(binary, key, max(1, mib << 7), columns, lines)
        payloads.append(Payload(key, name, data, warm_up))
    return payloads


def write_all(out, data):
    view = memoryview(data)
    while view:
        view = view[os.write(out, view[:CHUNK]):]


def measure(fd, out, data, timeout):
    wait_for_terminal(fd, timeout)
    start = time.perf_counter()
    write_all(out, data)
    wait_for_terminal(fd, timeout)
    return time.perf_counter() - start


def main():
    parser = argparse.ArgumentParser(description="Terminal-agnostic throughput benchmark.")
    parser.add_argument("--mib", type=int, default=16, help="payload size per workload (MiB)")
    parser.add_argument("--repeat", type=int, default=3, help="timed passes per workload")
    parser.add_argument("--only", action="append", help="run only these workloads (repeatable)")
    parser.add_argument("--timeout", type=float, default=120.0,
                        help="seconds to wait for each DA1 reply")
    parser.add_argument("--output", help="also append results to this file")
    parser.add_argument("--format", choices=["text", "json"], default="text")
    parser.add_argument("--pid", type=int, help="the terminal's process, if not found by itself")
    parser.add_argument("--settle", type=float, default=1.0,
                        help="seconds to wait after each workload before reading memory")
    args = parser.parse_args()
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        sys.exit("bench-throughput: stdin and stdout must be the terminal under test")

    fd, out = sys.stdin.fileno(), sys.stdout.fileno()
    size = shutil.get_terminal_size()
    try:
        payloads = build_payloads(args.mib, size.columns, size.lines, args.only)
    except (OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"bench-throughput: {error}")
    if args.only and len(payloads) != len(set(args.only)):
        sys.exit("bench-throughput: unknown workload; see `poopterm --benchmark --list`")
    pid = args.pid or terminal_pid(os.getpid())
    resident = lambda: mib((memory(pid) or {}).get("rss_kib")) if pid else None
    start = resident()
    results = []
    try:
        # Environment variables such as TERM_PROGRAM may come from a parent
        # terminal; the terminal's own XTVERSION answer is authoritative.
        terminal = parse_xtversion(wait_for_terminal(fd, args.timeout, b"\x1b[>q"))
        for key, name, data, warm_up in payloads:
            measure(fd, out, warm_up, args.timeout)
            seconds = [measure(fd, out, data, args.timeout) for _ in range(args.repeat)]
            rates = [len(data) / s / 1e6 for s in seconds]
            if pid:
                time.sleep(args.settle)
            results.append({
                "payload": key, "name": name, "bytes": len(data),
                "median_mb_s": statistics.median(rates), "min_mb_s": min(rates),
                "max_mb_s": max(rates), "rss_mib": resident(),
            })
    except NoReply as error:
        sys.exit(f"bench-throughput: {error}")
    finally:
        # Also delete kitty images, which survive a screen clear.
        os.write(out, b"\x1b_Ga=d,q=2\x1b\\\x1b[0m\x1b[r\x1b[2J\x1b[H")

    total_bytes = sum(r["bytes"] for r in results)
    total_seconds = sum(r["bytes"] / r["median_mb_s"] / 1e6 for r in results)
    report = {
        "terminal": terminal or "", "term": os.environ.get("TERM", ""),
        "term_program": os.environ.get("TERM_PROGRAM", ""),
        "columns": size.columns, "lines": size.lines, "repeat": args.repeat, "results": results,
        "overall_mb_s": total_bytes / total_seconds / 1e6 if results else 0,
        "pid": pid, "rss_start_mib": start,
        "peak_mib": mib((memory(pid) or {}).get("peak_kib")) if pid else None,
    }
    text = json.dumps(report, indent=2) if args.format == "json" else as_text(report, args.mib)
    print(text)
    if args.output:
        with open(args.output, "a") as handle:
            handle.write(text + "\n")


def as_text(report, mib_each):
    """The report as a table, memory in MiB where the terminal was found."""
    shown = lambda value: "-" if value is None else f"{value:.1f}"
    header = (f"{report['terminal'] or report['term_program'] or report['term']}  "
              f"{report['columns']}x{report['lines']}  {mib_each} MiB x {report['repeat']} "
              f"(median, min-max; resident MiB after each)")
    rows = [f"{r['payload']:<11} {r['median_mb_s']:9.1f} MB/s  "
            f"({r['min_mb_s']:.1f}-{r['max_mb_s']:.1f})  {shown(r['rss_mib']):>7}  {r['name']}"
            for r in report["results"]]
    memory = (f"memory      {shown(report['rss_start_mib'])} MiB at start, "
              f"{shown(report['peak_mib'])} MiB at peak (pid {report['pid'] or '-'})")
    return "\n".join([header, *rows, f"{'overall':<11} {report['overall_mb_s']:9.1f} MB/s", memory])


if __name__ == "__main__":
    main()
