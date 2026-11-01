"""End-to-end benchmark payloads: every write ends on a sequence boundary."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import tempfile
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('throughput', Path(__file__).parents[1] / 'scripts/bench-throughput.py')
throughput = importlib.util.module_from_spec(spec)
spec.loader.exec_module(throughput)


class Payloads(unittest.TestCase):
    def setUp(self):
        try:
            self.binary = throughput.poopterm_binary()
        except FileNotFoundError:
            self.skipTest('needs target/release/poopterm (cargo build --release)')

    def test_timed_and_warm_up_payloads_end_on_whole_sequences(self):
        # A payload cut inside `CSI 38;2;…` makes the following DA1 query part
        # of that sequence. kitty then drops both, never answers, and the
        # benchmark aborts (closing kitty's window, which looks like a crash).
        for payload in throughput.build_payloads(1, binary=self.binary):
            for data in [payload.data, payload.warm_up]:
                with self.subTest(payload=payload.key, size=len(data)):
                    self.assertIsNone(re.search(rb'\x1b(\[[0-9;:]*)?$', data))
                    self.assertTrue(payload.data.startswith(data))
            self.assertLess(len(payload.warm_up), len(payload.data))


    def test_the_terminal_names_itself_through_xtversion(self):
        # TERM_PROGRAM is inherited from the parent terminal, so kitty started
        # from ghostty would be reported as ghostty.
        reply = b'\x1bP>|kitty(0.49.2)\x1b\\\x1b[?62;c'
        self.assertEqual(throughput.parse_xtversion(reply), 'kitty(0.49.2)')
        self.assertIsNone(throughput.parse_xtversion(b'\x1b[?62;c'))

    def test_an_unterminated_xtversion_reply_is_rejected_in_linear_time(self):
        # A reply is untrusted: many openers without a terminator must not
        # rescan the rest of the reply from every opener.
        reply = b'\x1bP>|' * 200_000
        started = time.perf_counter()
        self.assertIsNone(throughput.parse_xtversion(reply))
        self.assertLess(time.perf_counter() - started, 0.5)
        self.assertEqual(throughput.parse_xtversion(b'x\x1bP>|a\x1bP>|b\x1b\\'), 'a\x1bP>|b')


class BinaryResolution(unittest.TestCase):
    def test_installed_binary_is_used_when_no_local_release_build_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / 'bin/poopterm'
            installed.parent.mkdir()
            installed.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {}, clear=True):
                    with mock.patch.object(shutil, 'which', return_value=str(installed)):
                        self.assertEqual(throughput.poopterm_binary(), installed)

    def test_local_release_binary_takes_precedence_over_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            local = root / 'target/release/poopterm'
            local.parent.mkdir(parents=True)
            local.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {}, clear=True):
                    with mock.patch.object(shutil, 'which', return_value='/usr/bin/poopterm'):
                        self.assertEqual(throughput.poopterm_binary(), local)

    def test_explicit_binary_takes_precedence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            explicit = root / 'custom/poopterm'
            explicit.parent.mkdir()
            explicit.touch()
            with mock.patch.object(throughput, 'ROOT', root):
                with mock.patch.dict(os.environ, {'POOPTERM_BIN': str(explicit)}):
                    with mock.patch.object(shutil, 'which', return_value='/usr/bin/poopterm'):
                        self.assertEqual(throughput.poopterm_binary(), explicit)


if __name__ == '__main__':
    unittest.main()


class TerminalProcess(unittest.TestCase):
    # pid -> (parent, controlling terminal), as `proc_stat` reports it.
    TREE = {
        40: (30, 'pts/3'),  # this script
        30: (20, 'pts/3'),  # the shell
        20: (10, '?'),      # the terminal emulator
        10: (1, '?'),       # the desktop session
    }

    def test_the_terminal_is_the_first_ancestor_off_our_terminal(self):
        self.assertEqual(throughput.terminal_pid(40, stat=self.TREE.get), 20)

    def test_a_multiplexer_between_counts_as_the_terminal(self):
        # tmux's server holds its own pty, so its client's terminal is not
        # ours either: memory is reported for what serves our pty.
        tree = {**self.TREE, 30: (25, 'pts/3'), 25: (20, 'pts/1')}
        self.assertEqual(throughput.terminal_pid(40, stat=tree.get), 25)

    def test_no_terminal_when_the_chain_ends_on_our_terminal(self):
        # Run from a console or ssh session whose ancestors share the tty.
        tree = {40: (30, 'tty1'), 30: (1, 'tty1')}
        self.assertIsNone(throughput.terminal_pid(40, stat=tree.get))

    def test_no_terminal_when_an_ancestor_vanished(self):
        tree = {40: (30, 'pts/3')}
        self.assertIsNone(throughput.terminal_pid(40, stat=tree.get))
        self.assertIsNone(throughput.terminal_pid(40, stat={}.get))

    def test_a_parent_loop_ends_the_search(self):
        tree = {40: (30, 'pts/3'), 30: (40, 'pts/3')}
        self.assertIsNone(throughput.terminal_pid(40, stat=tree.get))

    def test_a_program_name_with_parentheses_does_not_shift_the_fields(self):
        stat = '77 (a) S 9 (b) R 41 77 77 34819 77 4194560\n'
        with mock.patch.object(throughput.Path, 'read_text', return_value=stat):
            self.assertEqual(throughput.proc_stat(77), (41, '34819'))

    def test_this_process_has_a_readable_stat(self):
        parent, _ = throughput.proc_stat(os.getpid())
        self.assertEqual(parent, os.getppid())


class ProcessMemory(unittest.TestCase):
    STATUS = 'Name:\tpoopterm\nVmPeak:\t 900000 kB\nVmHWM:\t   81234 kB\nVmRSS:\t   40960 kB\nThreads:\t9\n'

    def test_status_gives_resident_and_peak_kib(self):
        self.assertEqual(throughput.parse_status(self.STATUS), {'rss_kib': 40960, 'peak_kib': 81234})

    def test_missing_fields_are_unknown_not_zero(self):
        # Kernel threads and zombies have no Vm* lines.
        self.assertEqual(throughput.parse_status('Name:\tkworker\nState:\tZ\n'),
                         {'rss_kib': None, 'peak_kib': None})

    def test_this_process_reports_some_memory(self):
        own = throughput.memory(os.getpid())
        self.assertGreater(own['rss_kib'], 0)
        if Path('/proc/self/status').exists():
            self.assertGreaterEqual(own['peak_kib'], own['rss_kib'])

    def test_a_gone_process_reports_nothing(self):
        child = os.fork() if hasattr(os, 'fork') else None
        if child == 0:
            os._exit(0)
        os.waitpid(child, 0)
        self.assertIsNone(throughput.memory(child))
        self.assertIsNone(throughput.proc_stat(child))

    def test_mib_rounds_to_a_tenth(self):
        self.assertEqual(throughput.mib(40960), 40.0)
        self.assertEqual(throughput.mib(1100), 1.1)
        self.assertIsNone(throughput.mib(None))


class TextReport(unittest.TestCase):
    def report(self, **memory):
        report = {
            'terminal': 'poopterm 26.10.137', 'term_program': None, 'term': 'xterm-256color',
            'columns': 120, 'lines': 40, 'repeat': 5, 'overall_mb_s': 512.0,
            'results': [{'payload': 'ascii', 'name': 'plain text', 'median_mb_s': 900.0,
                         'min_mb_s': 850.5, 'max_mb_s': 950.0, 'rss_mib': 42.5}],
            'pid': 20, 'rss_start_mib': 30.0, 'peak_mib': 88.8,
        }
        report.update(memory)
        return throughput.as_text(report, 64)

    def test_memory_is_shown_after_each_workload_and_at_the_end(self):
        lines = self.report().splitlines()
        self.assertIn('poopterm 26.10.137  120x40  64 MiB x 5', lines[0])
        self.assertRegex(lines[1], r'^ascii +900\.0 MB/s +\(850\.5-950\.0\) +42\.5 +plain text$')
        self.assertRegex(lines[2], r'^overall +512\.0 MB/s$')
        self.assertEqual(lines[3], 'memory      30.0 MiB at start, 88.8 MiB at peak (pid 20)')

    def test_unknown_memory_shows_a_dash(self):
        results = [{'payload': 'ascii', 'name': 'plain text', 'median_mb_s': 1.0,
                    'min_mb_s': 1.0, 'max_mb_s': 1.0, 'rss_mib': None}]
        lines = self.report(pid=None, rss_start_mib=None, peak_mib=None, results=results).splitlines()
        self.assertRegex(lines[1], r'\) +- +plain text$')
        self.assertEqual(lines[3], 'memory      - MiB at start, - MiB at peak (pid -)')
