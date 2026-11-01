"""The window benchmark of frame times and typing latency: its screens, the
half that runs inside the terminal, and the summary it prints."""
import argparse
import fcntl
import importlib.util
import io
import json
import os
from pathlib import Path
import pty
import re
import select
import subprocess
import struct
import sys
import termios
import tempfile
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('frames', Path(__file__).parents[1] / 'scripts/bench-frames.py')
frames = importlib.util.module_from_spec(spec)
spec.loader.exec_module(frames)

SCRIPT = str(Path(__file__).parents[1] / 'scripts/bench-frames.py')


def report(total, paint, show, latency=()):
    """A report whose samples are the given lists."""
    return {
        "frames": {"software": len(total), "gpu": 0, "gpu_whole": 0}, "size": [1000, 600], "scale": 1,
        "total_us": list(total), "paint_us": list(paint), "show_us": list(show), "latency_us": list(latency),
    }


# A stand-in for poopterm: makes its shell ready (or fails to, with
# FAKE_EXIT set) and writes stats that show what it was started with.
FAKE_TERMINAL = '''#!{python}
import json, os, shlex, sys
words = shlex.split(open(os.environ["SHELL"]).read().splitlines()[1])
if os.environ.get("FAKE_EXIT"):
    sys.exit(1)
open(words[-1], "w").close()
inside = words[words.index("--inside") + 1:]
with open(os.environ["POOPTERM_FRAME_STATS"], "w") as out:
    json.dump({{"renderer": os.environ["POOPTERM_RENDERER"], "inside": inside, "total_us": [1.5]}}, out)
'''


class Sizes(unittest.TestCase):
    def test_reads_width_by_height(self):
        self.assertEqual(frames.parse_size('1000x600'), (1000, 600))
        self.assertEqual(frames.parse_size('1x1'), (1, 1))

    def test_refuses_anything_else(self):
        for text in ['1000', '1000x', 'x600', '0x600', '1000x0', '-5x600', '10.5x600', '1000X600', '']:
            with self.subTest(text=text), self.assertRaises(argparse.ArgumentTypeError):
                frames.parse_size(text)


class Screens(unittest.TestCase):
    def cells(self, data):
        """The characters of each row of a screen, without its escapes."""
        text = data.decode()
        self.assertTrue(text.startswith('\x1b[H'), 'the screen starts at home')
        self.assertTrue(text.endswith('\x1b[m'), 'the screen resets its colors')
        return [re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', row) for row in text.split('\r\n')]

    def test_fills_every_cell_without_scrolling(self):
        rows = self.cells(frames.screen(0, 90, 26))
        self.assertEqual(len(rows), 26)
        self.assertTrue(all(len(row) == 90 for row in rows))
        # Rows are joined, not ended: a newline after the last would scroll.
        self.assertFalse(frames.screen(0, 90, 26).endswith(b'\n'))

    def test_every_cell_changes_from_one_screen_to_the_next(self):
        for index in [0, 1, 25, 26, 299]:
            before, after = self.cells(frames.screen(index, 90, 26)), self.cells(frames.screen(index + 1, 90, 26))
            for row, (old, new) in enumerate(zip(before, after)):
                with self.subTest(index=index, row=row):
                    self.assertTrue(all(a != b for a, b in zip(old, new)))

    def test_every_row_has_a_color_and_the_colors_change(self):
        for index in [0, 6, 7]:
            colors = re.findall(r'\x1b\[(3[1-7])m', frames.screen(index, 4, 10).decode())
            following = re.findall(r'\x1b\[(3[1-7])m', frames.screen(index + 1, 4, 10).decode())
            self.assertEqual(len(colors), 10)
            self.assertTrue(all(a != b for a, b in zip(colors, following)))

    def test_one_cell(self):
        self.assertEqual(self.cells(frames.screen(0, 1, 1)), ['a'])


class Replies(unittest.TestCase):
    def setUp(self):
        self.read, self.write = os.pipe()
        self.addCleanup(os.close, self.read)
        self.addCleanup(os.close, self.write)

    def test_reads_a_reply_split_over_reads(self):
        os.write(self.write, b'\x1b[?62;')
        reader = mock.patch.object(frames.os, 'read', side_effect=[b'\x1b[?62;', b'22c'])
        with reader, mock.patch.object(frames.select, 'select', return_value=([self.read], [], [])):
            self.assertEqual(frames.wait_for_reply(self.read), b'\x1b[?62;22c')

    def test_reads_a_whole_reply(self):
        os.write(self.write, b'\x1b[?62;22c')
        self.assertEqual(frames.wait_for_reply(self.read, timeout=1), b'\x1b[?62;22c')

    def test_gives_up_without_a_reply(self):
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            frames.wait_for_reply(self.read, timeout=0.1)
        self.assertLess(time.monotonic() - started, 2)

    def test_gives_up_on_a_reply_that_never_ends(self):
        os.write(self.write, b'\x1b[?62;')
        with self.assertRaises(TimeoutError):
            frames.wait_for_reply(self.read, timeout=0.1)


class Inside(unittest.TestCase):
    """The half that runs in the terminal, under a pty that plays the
    terminal: it answers DA1 and types keys."""

    def spawn(self, frame_count, keys, ready):
        pid, fd = pty.fork()
        if pid == 0:
            # A new pty is 0 by 0 cells; the terminal under test has a size.
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
            os.execv(sys.executable, [sys.executable, SCRIPT, '--inside', str(frame_count), str(keys), ready])
        self.addCleanup(os.close, fd)
        return pid, fd

    def read_until(self, fd, end, timeout=10):
        data = b''
        deadline = time.monotonic() + timeout
        while not data.endswith(end):
            left = deadline - time.monotonic()
            self.assertGreater(left, 0, f'no {end!r} in {data[-200:]!r}')
            if select.select([fd], [], [], left)[0]:
                data += os.read(fd, 65536)
        return data

    def finish(self, pid):
        _, status = os.waitpid(pid, 0)
        self.assertEqual(os.waitstatus_to_exitcode(status), 0)

    def test_paints_each_screen_after_the_last_was_answered_then_echoes_keys(self):
        with tempfile.TemporaryDirectory() as scratch:
            ready = os.path.join(scratch, 'ready')
            pid, fd = self.spawn(3, 2, ready)
            for index in range(3):
                data = self.read_until(fd, b'\x1b[c')
                self.assertIn(frames.screen(index, 80, 24)[:40], data)
                self.assertFalse(os.path.exists(ready), 'ready only after the last screen')
                os.write(fd, b'\x1b[?62;22c')
            deadline = time.monotonic() + 10
            while not os.path.exists(ready):
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.01)
            os.write(fd, b'q')
            self.assertEqual(self.read_until(fd, b'q'), b'q')
            os.write(fd, b'\r')
            # Raw mode: a carriage return comes back as typed, not as a newline.
            self.assertEqual(self.read_until(fd, b'\r'), b'\r')
            self.finish(pid)

    def test_without_frames_it_is_ready_at_once(self):
        with tempfile.TemporaryDirectory() as scratch:
            ready = os.path.join(scratch, 'ready')
            pid, fd = self.spawn(0, 1, ready)
            deadline = time.monotonic() + 10
            while not os.path.exists(ready):
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.01)
            os.write(fd, b'z')
            self.assertEqual(self.read_until(fd, b'z'), b'z')
            self.finish(pid)


class Launching(unittest.TestCase):
    def test_the_shell_runs_the_inside_half(self):
        with tempfile.TemporaryDirectory() as scratch:
            ready = os.path.join(scratch, "it's ready")
            script = frames.shell_script(sys.executable, SCRIPT, 5, 7, ready)
            self.assertTrue(script.startswith('#!/bin/sh\nexec '))
            shell = Path(scratch, 'shell.sh')
            shell.write_text(script.replace(' --inside ', ' -c "import sys; print(sys.argv[1:])" --inside ', 1)
                             .replace(f'{sys.executable} {SCRIPT}', sys.executable))
            shell.chmod(0o755)
            out = subprocess.run([str(shell)], capture_output=True, text=True, check=True).stdout
            # The path survives the shell, quote and all.
            self.assertEqual(out.strip(), str(['--inside', '5', '7', ready]))

    def test_floats_and_sizes_the_window_of_one_process(self):
        command = frames.sway_command(4242, 1000, 600)
        self.assertEqual(command[0], 'swaymsg')
        self.assertEqual(command[1], '[pid=4242] floating enable, resize set 1000 600, move position 0 0')



class Running(unittest.TestCase):
    """One launch, with a fake terminal, and swaymsg and wtype recorded
    instead of run."""

    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.scratch = scratch.name
        self.binary = Path(self.scratch, 'poopterm')
        self.binary.write_text(FAKE_TERMINAL.format(python=sys.executable))
        self.binary.chmod(0o755)
        self.commands = []
        real = subprocess.run

        def record(command, **kwargs):
            if command[0] in ('swaymsg', 'wtype'):
                self.commands.append(command)
                return subprocess.CompletedProcess(command, 0)
            return real(command, **kwargs)

        for patch in [mock.patch.object(frames.subprocess, 'run', record),
                      mock.patch.object(frames.time, 'sleep', lambda _: None)]:
            patch.start()
            self.addCleanup(patch.stop)

    def test_returns_the_stats_its_terminal_wrote(self):
        stats = frames.run(str(self.binary), 'gpu', (800, 500), 12, 3, self.scratch)
        self.assertEqual(stats['renderer'], 'gpu')
        self.assertEqual(stats['inside'][:2], ['12', '3'])
        self.assertEqual(stats['total_us'], [1.5])

    def test_sizes_the_window_then_types_the_keys(self):
        frames.run(str(self.binary), 'software', (800, 500), 0, 3, self.scratch)
        self.assertEqual(self.commands[0][0], 'swaymsg')
        self.assertIn('resize set 800 500', self.commands[0][1])
        self.assertEqual(self.commands[1:], [['wtype', 'a'], ['wtype', 'b'], ['wtype', 'c']])

    def test_files_left_by_the_last_run_do_not_count(self):
        frames.run(str(self.binary), 'software', (800, 500), 0, 0, self.scratch)
        # This terminal exits without making its shell ready or writing
        # stats: the files of the run before must not stand in for them.
        with mock.patch.dict(os.environ, FAKE_EXIT='1'), self.assertRaisesRegex(RuntimeError, 'never got ready'):
            frames.run(str(self.binary), 'gpu', (800, 500), 0, 0, self.scratch)


class Summaries(unittest.TestCase):
    def test_percentiles_are_nearest_ranks(self):
        samples = list(range(20, 0, -1))
        self.assertEqual(frames.percentile(samples, 0.5), 10)
        self.assertEqual(frames.percentile(samples, 0.95), 19)
        self.assertEqual(frames.percentile([5, 1, 9], 0.5), 5)
        self.assertEqual(frames.percentile([5, 1, 9], 0.95), 9)
        self.assertEqual(frames.percentile([7], 0.95), 7)
        self.assertEqual(samples, list(range(20, 0, -1)), 'the samples are left as they were')

    def test_medians_of_run_medians_with_their_bounds(self):
        runs = [report([300, 100, 500], [100] * 3, [50] * 3, [9000, 1000, 20000]),
                report([100, 100, 100], [300] * 3, [70] * 3, [7000] * 3),
                report([200], [200], [60], [8000, 8000])]
        summary = frames.summarize(runs)
        self.assertEqual(summary['Frame'], (200, 100, 300))
        self.assertEqual(summary['Paint'], (200, 100, 300))
        self.assertEqual(summary['Show'], (60, 50, 70))
        self.assertEqual(summary['Latency'], (8000, 7000, 9000))
        self.assertEqual(summary['Latency p95'], (8000, 7000, 20000))

    def test_an_even_count_of_runs_takes_the_middle_mean(self):
        summary = frames.summarize([report([100], [1], [1]), report([300], [1], [1])])
        self.assertEqual(summary['Frame'], (200, 100, 300))

    def test_measures_without_samples_are_left_out(self):
        summary = frames.summarize([report([100], [10], [5]), report([200], [], [6])])
        self.assertEqual(summary['Paint'], (10, 10, 10))
        self.assertNotIn('Latency', summary)
        self.assertNotIn('Latency p95', summary)
        self.assertEqual(frames.summarize([]), {})

    def test_milliseconds_with_their_spread(self):
        self.assertEqual(frames.ms((1234.5, 1000, 2000)), '1.23 (1.00–2.00)')
        self.assertEqual(frames.ms((0, 0, 0)), '0.00 (0.00–0.00)')

    def test_a_row_per_build_and_renderer(self):
        rows = frames.table({
            ('poopterm', 'software'): frames.summarize([report([2000], [1500], [400], [9000])]),
            ('poopterm', 'gpu'): frames.summarize([report([1000], [300], [600])]),
        }).splitlines()
        self.assertEqual(len(rows), 4)
        self.assertEqual(rows[0], '| Build | Renderer | Frame | Paint | Show | Latency | Latency p95 |')
        self.assertEqual(rows[0].count('|'), rows[1].count('|'))
        self.assertEqual(rows[2], '| poopterm | software | 2.00 (2.00–2.00) | 1.50 (1.50–1.50) | 0.40 (0.40–0.40) '
                                  '| 9.00 (9.00–9.00) | 9.00 (9.00–9.00) |')
        self.assertTrue(rows[3].startswith('| poopterm | gpu | 1.00'))
        self.assertTrue(rows[3].endswith('| – | – |'))


class Main(unittest.TestCase):
    def test_alternates_renderers_and_builds_on_every_run(self):
        calls = []

        def run(binary, renderer, size, frame_count, keys, scratch):
            calls.append((binary, renderer, size, frame_count, keys))
            return report([1000], [500], [300], [8000] if keys else [])

        argv = ['--binary', '/a/before', '--binary', '/b/after', '--runs', '2', '--size', '800x500',
                '--frames', '10', '--keys', '4']
        stdout = io.StringIO()
        with mock.patch.object(frames, 'run', run), mock.patch('sys.stdout', stdout), \
                mock.patch('sys.stderr', io.StringIO()):
            self.assertEqual(frames.main(argv), 0)
        order = [(binary, renderer) for binary, renderer, *_ in calls[::2]]
        self.assertEqual(order, [('/a/before', 'software'), ('/a/before', 'gpu'), ('/b/after', 'software'),
                                 ('/b/after', 'gpu')] * 2)
        # Each pairs a run of frames without keys with a run of keys without frames.
        self.assertTrue(all(call[2:] == ((800, 500), 10, 0) for call in calls[::2]))
        self.assertTrue(all(call[2:] == ((800, 500), 0, 4) for call in calls[1::2]))
        table = stdout.getvalue()
        self.assertIn('| before | software | 1.00 (1.00–1.00) | 0.50 (0.50–0.50) | 0.30 (0.30–0.30) '
                      '| 8.00 (8.00–8.00) |', table)
        self.assertIn('| after | gpu |', table)

    def test_measures_the_release_build_by_default(self):
        binaries = []

        def run(binary, *_):
            binaries.append(binary)
            return report([1], [1], [1])

        with mock.patch.object(frames, 'run', run), mock.patch('sys.stdout', io.StringIO()), \
                mock.patch('sys.stderr', io.StringIO()):
            frames.main(['--runs', '1', '--renderer', 'software'])
        self.assertTrue(all(binary.endswith('/target/release/poopterm') for binary in binaries))
        self.assertTrue(Path(binaries[0]).is_absolute())

    def test_one_renderer_when_asked(self):
        calls = []

        def run(binary, renderer, *_):
            calls.append(renderer)
            return report([1], [1], [1], [1])

        with mock.patch.object(frames, 'run', run), mock.patch('sys.stdout', io.StringIO()), \
                mock.patch('sys.stderr', io.StringIO()):
            frames.main(['--binary', 'x', '--renderer', 'gpu', '--runs', '1'])
        self.assertEqual(calls, ['gpu', 'gpu'])

    def test_refuses_an_unknown_renderer(self):
        with mock.patch('sys.stderr', io.StringIO()), self.assertRaises(SystemExit):
            frames.main(['--renderer', 'metal'])


if __name__ == '__main__':
    unittest.main()
