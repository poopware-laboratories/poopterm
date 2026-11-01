#!/usr/bin/env python3
"""Frame time and typing latency of Poopterm's renderers, in real windows.

Launches poopterm (sway only: windows are floated and sized with swaymsg,
keys typed with wtype) with each renderer in turn, alternating, and reads
the summary each run writes to POOPTERM_FRAME_STATS on exit:

* Frames: the shell paints a different full screen about 30 times a
  second, waiting for each to be parsed (a DA1 reply), so every frame
  redraws the whole grid. Reported: time in software painting, in showing
  the frame (GPU recording, submission and presentation, or the software
  presenter), and in all of the frame.
* Latency: wtype types keys into a raw-mode reader that echoes each one,
  as a shell would; measured from the key event to the end of the first
  frame drawn after the echo arrived (not to photons on the glass).

Usage:

    python3 scripts/bench-frames.py                          # software and gpu, 3 runs
    python3 scripts/bench-frames.py --runs 5 --size 1600x1000 --binary /tmp/poopterm-before
"""
import argparse
import json
import math
import os
from pathlib import Path
import select
import shlex
import statistics
import subprocess
import sys
import tempfile
import termios
import time
import tty

RENDERERS = ("software", "gpu")
COLUMNS = ("Frame", "Paint", "Show", "Latency", "Latency p95")
# Table column, report measure, and the percentile taken of each run.
MEASURES = tuple(zip(COLUMNS, ("total_us", "paint_us", "show_us", "latency_us", "latency_us"),
                     (0.5, 0.5, 0.5, 0.5, 0.95)))


def parse_size(text):
    width, _, height = text.partition("x")
    if not (width.isdigit() and height.isdigit() and int(width) > 0 and int(height) > 0):
        raise argparse.ArgumentTypeError(f"expected WIDTHxHEIGHT, got {text!r}")
    return int(width), int(height)


def screen(index, columns, lines):
    """A full screen that differs from the screens before and after it in
    every cell: the cursor goes home, every row is rewritten, and the last
    row ends without a newline so nothing scrolls."""
    rows = []
    for row in range(lines):
        color = 31 + (index + row) % 7
        start = (index * 7 + row * 3) % 26
        text = "".join(chr(ord("a") + (start + column) % 26) for column in range(columns))
        rows.append(f"\x1b[{color}m{text}")
    return ("\x1b[H" + "\r\n".join(rows) + "\x1b[m").encode()


def wait_for_reply(fd, timeout=5.0):
    """Reads until the end of a DA1 reply (`CSI ? … c`)."""
    reply = b""
    deadline = time.monotonic() + timeout
    while not reply.endswith(b"c"):
        left = deadline - time.monotonic()
        if left <= 0 or not select.select([fd], [], [], left)[0]:
            raise TimeoutError("the terminal did not answer DA1")
        reply += os.read(fd, 64)
    return reply


def inside(frames, keys, ready, interval=1 / 30):
    """Runs in the terminal under test: paints `frames` screens, then echoes
    `keys` typed keys, after touching `ready`."""
    fd = sys.stdin.fileno()
    saved = termios.tcgetattr(fd)
    # Raw before saying ready: setraw flushes input typed until then.
    tty.setraw(fd)
    try:
        columns, lines = os.get_terminal_size(fd)
        out = sys.stdout.buffer
        for index in range(frames):
            out.write(screen(index, columns, lines) + b"\x1b[c")
            out.flush()
            wait_for_reply(fd)
            time.sleep(interval)
        Path(ready).touch()
        for _ in range(keys):
            if not select.select([fd], [], [], 30)[0]:
                break
            out.write(os.read(fd, 1))
            out.flush()
    finally:
        termios.tcsetattr(fd, termios.TCSAFLUSH, saved)


def shell_script(python, script, frames, keys, ready):
    """The login shell for a run: the inside half of this script."""
    command = [python, script, "--inside", str(frames), str(keys), str(ready)]
    return "#!/bin/sh\nexec " + " ".join(shlex.quote(part) for part in command) + "\n"


def sway_command(pid, width, height):
    return ["swaymsg", f"[pid={pid}] floating enable, resize set {width} {height}, move position 0 0"]


def run(binary, renderer, size, frames, keys, scratch, wait_ready=60, key_delay=0.12):
    """One launch of `binary`; its frame statistics."""
    scratch = Path(scratch)
    stats, ready, shell = scratch / "stats.json", scratch / "ready", scratch / "shell.sh"
    for path in (stats, ready):
        path.unlink(missing_ok=True)
    shell.write_text(shell_script(sys.executable, os.path.abspath(__file__), frames, keys, ready))
    shell.chmod(0o755)
    env = dict(os.environ, SHELL=str(shell), POOPTERM_RENDERER=renderer, POOPTERM_FRAME_STATS=str(stats))
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(0.8)
        subprocess.run(sway_command(proc.pid, *size), stdout=subprocess.DEVNULL, check=False)
        deadline = time.monotonic() + wait_ready + frames / 10
        while not ready.exists():
            if proc.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError(f"{binary} ({renderer}) never got ready")
            time.sleep(0.05)
        for index in range(keys):
            subprocess.run(["wtype", chr(ord("a") + index % 26)], check=True)
            time.sleep(key_delay)
        proc.wait(timeout=60)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    return json.loads(stats.read_text())


def percentile(samples, fraction):
    """The nearest-rank percentile of `samples`."""
    ordered = sorted(samples)
    return ordered[max(1, math.ceil(fraction * len(ordered))) - 1]


def summarize(reports):
    """Per measure, the median across runs of each run's median, with the
    smallest and largest run median; latency also with its p95s."""
    summary = {}
    for name, measure, fraction in MEASURES:
        values = [percentile(report[measure], fraction) for report in reports if report[measure]]
        if values:
            summary[name] = (statistics.median(values), min(values), max(values))
    return summary


def ms(spread):
    median, low, high = (value / 1000 for value in spread)
    return f"{median:.2f} ({low:.2f}–{high:.2f})"


def table(summaries):
    """A Markdown table, one row per (binary, renderer), times in ms."""
    lines = ["| Build | Renderer | " + " | ".join(COLUMNS) + " |", "|---|---|" + "---:|" * len(COLUMNS)]
    for (build, renderer), summary in summaries.items():
        cells = [ms(summary[name]) if name in summary else "–" for name in COLUMNS]
        lines.append(f"| {build} | {renderer} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--inside", nargs=3, metavar=("FRAMES", "KEYS", "READY"), help=argparse.SUPPRESS)
    parser.add_argument("--binary", action="append", help="poopterm to measure (repeatable; "
                        "default target/release/poopterm)")
    parser.add_argument("--renderer", action="append", choices=RENDERERS, help="repeatable")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--size", type=parse_size, default=(1000, 600), help="window size, WIDTHxHEIGHT")
    parser.add_argument("--frames", type=int, default=300)
    parser.add_argument("--keys", type=int, default=40)
    args = parser.parse_args(argv)
    if args.inside:
        frames, keys, ready = args.inside
        inside(int(frames), int(keys), ready)
        return 0
    binaries = args.binary or [str(Path(__file__).resolve().parents[1] / "target/release/poopterm")]
    renderers = args.renderer or list(RENDERERS)
    reports = {}
    with tempfile.TemporaryDirectory(prefix="bench-frames-") as scratch:
        for index in range(args.runs):
            for binary in binaries:
                for renderer in renderers:
                    key = (binary, renderer)
                    frames = run(binary, renderer, args.size, args.frames, 0, scratch)
                    typing = run(binary, renderer, args.size, 0, args.keys, scratch)
                    reports.setdefault(key, []).append({**frames, "latency_us": typing["latency_us"]})
                    print(f"run {index + 1}/{args.runs}: {binary} {renderer}", file=sys.stderr)
    summaries = {(Path(binary).name, renderer): summarize(runs) for (binary, renderer), runs in reports.items()}
    print(table(summaries))
    return 0


if __name__ == "__main__":
    sys.exit(main())
