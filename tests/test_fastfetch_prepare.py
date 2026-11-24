"""ios/packages/fastfetch/prepare.py: what the iOS build of fastfetch swaps."""
import importlib.util
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent / "ios" / "packages" / "fastfetch"
spec = importlib.util.spec_from_file_location("prepare", HERE / "prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class PrepareTest(unittest.TestCase):
    def test_the_apple_cpu_detector_is_replaced_by_ours(self):
        self.assertIsNone(prepare.REPLACE["src/detection/cpu/cpu_apple.c"])
        self.assertIn("cpu_ios.m", prepare.OURS)

    def test_every_file_of_ours_exists(self):
        for name in prepare.OURS:
            self.assertTrue((HERE / "src" / name).is_file(), name)

    def test_the_replaced_source_drops_out_of_the_ios_list(self):
        block = "    src/detection/cpu/cpu_apple.c\n    src/detection/gpu/gpu_apple.m\n    src/detection/os/os_apple.m\n"
        sources = prepare.ios_sources(block)
        self.assertNotIn("src/detection/cpu/cpu_apple.c", sources)
        self.assertIn("src/detection/os/os_apple.m", sources)

    def test_the_cpu_is_named_from_the_gpu_and_falls_back_to_the_model(self):
        text = (HERE / "src" / "cpu_ios.m").read_text()
        self.assertIn('" GPU"', text)
        self.assertIn("hw.machine", text)
        self.assertIn("MTLCreateSystemDefaultDevice", text)


if __name__ == "__main__":
    unittest.main()
