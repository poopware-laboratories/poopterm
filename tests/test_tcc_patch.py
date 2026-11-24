"""ios/packages/tcc/tcc-ios.patch: the changes to tinycc's sources."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATCH = ROOT / "ios" / "packages" / "tcc" / "tcc-ios.patch"
SOURCE = ROOT / "target" / "ios-ports" / "tinycc"


class TccPatchTest(unittest.TestCase):
    def test_it_touches_only_the_run_and_support_code(self):
        files = [line.split()[1] for line in PATCH.read_text().splitlines() if line.startswith("+++")]
        self.assertEqual(sorted(f.split("/", 1)[1] for f in files), ["libtcc.c", "tccrun.c"])

    def test_the_runtime_comes_from_sources_not_prebuilt_files(self):
        text = PATCH.read_text()
        for source in ("runmain.c", "libtcc1.c", "lib-arm64.c", "builtin.c"):
            self.assertIn(source, text)
        self.assertIn("POOPTERM_RUNTIME_SOURCES", text)

    def test_code_goes_to_the_apps_region_through_a_full_width_view(self):
        text = PATCH.read_text()
        self.assertIn("poopterm_jit_dual", text)
        self.assertIn("addr_t ptr_diff", text)
        self.assertNotIn("unsigned ptr_diff", text.replace("-static int tcc_relocate_ex(TCCState *s1, void *ptr, unsigned ptr_diff)", ""))

    def test_every_hook_the_patch_calls_is_defined_by_us(self):
        sources = (ROOT / "ios" / "packages" / "tcc" / "ptcc_main.c").read_text()
        shim = (ROOT / "ios" / "commands" / "poopterm_cmd.c").read_text()
        self.assertIn("void poopterm_tcc_symbols", sources)
        self.assertIn("int poopterm_jit_dual", shim)
        self.assertIn("int poopterm_jit_owns", shim)

    @unittest.skipUnless(SOURCE.is_dir(), "needs target/ios-ports/tinycc")
    def test_it_applies_cleanly_to_the_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "src"
            shutil.copytree(SOURCE, copy, ignore=shutil.ignore_patterns(".git"))
            done = subprocess.run(
                ["patch", "-p1", "--dry-run"], input=PATCH.read_text(), cwd=copy, capture_output=True, text=True
            )
            self.assertEqual(done.returncode, 0, done.stdout + done.stderr)


if __name__ == "__main__":
    unittest.main()
