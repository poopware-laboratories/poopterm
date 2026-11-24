"""ios/terminfo.sh: the app's own copy of the terminal descriptions."""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "ios" / "terminfo.sh"
HOST = Path("/usr/share/terminfo")


@unittest.skipUnless(HOST.is_dir(), "needs the build machine's terminfo")
class TerminfoBundleTest(unittest.TestCase):
    def run_script(self, out, **env):
        return subprocess.run(
            [str(SCRIPT), str(out)],
            capture_output=True,
            text=True,
            env={**os.environ, **env},
        )

    def test_copies_the_entries_the_programs_need_in_hex_directories(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "terminfo"
            result = self.run_script(out)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((out / "78" / "xterm-256color").is_file())
            self.assertTrue((out / "76" / "vt100").is_file())
            self.assertTrue((out / "64" / "dumb").is_file())
            self.assertIn("28 files", result.stdout)
            self.assertTrue((out / "x" / "xterm-256color").is_file(), "ncurses built from source")
            self.assertEqual(
                (out / "x" / "xterm-256color").read_bytes(),
                (out / "78" / "xterm-256color").read_bytes(),
            )

    @unittest.skipUnless(shutil.which("infocmp"), "needs infocmp")
    def test_the_copied_entry_reads_back_as_a_256_color_xterm(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "terminfo"
            self.run_script(out)
            shown = subprocess.run(
                ["infocmp", "-A", str(out), "xterm-256color"],
                capture_output=True,
                text=True,
            )
            self.assertEqual(shown.returncode, 0, shown.stderr)
            self.assertIn("colors#0x100", shown.stdout.replace("colors#256", "colors#0x100"))

    def test_the_bundle_is_small(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "terminfo"
            self.run_script(out)
            size = sum(f.stat().st_size for f in out.rglob("*") if f.is_file())
            self.assertLess(size, 128 * 1024)

    def test_replaces_what_was_there(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "terminfo"
            out.mkdir()
            (out / "stale").write_text("old")
            self.run_script(out)
            self.assertFalse((out / "stale").exists())

    def test_a_missing_entry_fails_and_names_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            empty = Path(tmp) / "empty"
            empty.mkdir()
            result = self.run_script(Path(tmp) / "out", TERMINFO_SOURCE=str(empty))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("xterm-256color", result.stderr)


if __name__ == "__main__":
    unittest.main()
