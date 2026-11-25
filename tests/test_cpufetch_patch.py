"""ios/packages/cpufetch/patch.py: teaching cpufetch the Apple chips its
tables do not know, in a copy of its sources."""
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATCH = ROOT / "ios" / "packages" / "cpufetch" / "patch.py"
SOURCE = ROOT / "target" / "ios-ports" / "cpufetch" / "src"

# The texts patch.py replaces, as cpufetch 1.06 has them.
MIDR = """struct cpuInfo* get_cpu_info_mach(struct cpuInfo* cpu) {
  uint32_t cpu_family = get_sys_info_by_name("hw.cpufamily");
  if(cpu_family == CPUFAMILY_ARM_FIRESTORM_ICESTORM) {
    fill_cpu_info_firestorm_icestorm(cpu, pcores, ecores);
  }
  else {
    printBugCheckRelease("Found invalid cpu_family: 0x%.8X", cpu_family);
    return NULL;
  }

  return cpu;
}
"""
SOC = """struct system_on_chip* guess_soc_apple(struct system_on_chip* soc) {
  if(cpu_family == CPUFAMILY_ARM_FIRESTORM_ICESTORM) {
    fill_soc(soc, "M1", SOC_APPLE_M1, 5);
  }
  else {
    printBugCheckRelease("Found invalid cpu_family: 0x%.8X", cpu_family);
    soc->soc_vendor = SOC_VENDOR_UNKNOWN;
  }
  return soc;
}
"""
UARCH = "void fill_uarch(struct uarch* arch, struct cpuInfo* cpu, char* str, MICROARCH u, VENDOR vendor) {}\n"


def tree(directory, midr=MIDR, soc=SOC):
    arm = Path(directory) / "arm"
    arm.mkdir(parents=True)
    (arm / "midr.c").write_text(midr)
    (arm / "soc.c").write_text(soc)
    (arm / "uarch.c").write_text(UARCH)
    return Path(directory)


def patch(directory):
    return subprocess.run([sys.executable, "-I", str(PATCH), str(directory)],
                          capture_output=True, text=True)


class CpufetchPatchTest(unittest.TestCase):
    def test_an_unknown_family_fills_two_clusters_instead_of_failing(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = tree(tmp)
            self.assertEqual(patch(src).returncode, 0)
            midr = (src / "arm" / "midr.c").read_text()
            self.assertNotIn("return NULL;", midr)
            self.assertIn("poop_fill_cpu_info_apple(cpu, pcores, ecores);", midr)
            self.assertIn("cpu->peak_performance = -1;", midr, "no frequency, no peak")
            # Declared before get_cpu_info_mach, which calls it.
            self.assertLess(midr.index("static void poop_fill_cpu_info_apple"),
                            midr.index("struct cpuInfo* get_cpu_info_mach"))
            # Efficiency cores first, then performance, as cpufetch lists them.
            fill = midr[midr.index("static void poop_fill_cpu_info_apple"):]
            self.assertLess(fill.index("poop_fill_cluster(cpu, ecores)"),
                            fill.index("poop_fill_cluster(cpu->next_cpu, pcores)"))

    def test_the_chip_is_apples_by_the_name_metal_gives(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = tree(tmp)
            self.assertEqual(patch(src).returncode, 0)
            soc = (src / "arm" / "soc.c").read_text()
            self.assertTrue(soc.startswith("char* poop_apple_chip_name(void);\n"))
            self.assertIn("soc->soc_vendor = SOC_VENDOR_APPLE;", soc)
            self.assertIn("soc->raw_name = poop_apple_chip_name();", soc)
            self.assertNotIn("SOC_VENDOR_UNKNOWN", soc)
            chip = (ROOT / "ios" / "packages" / "cpufetch" / "apple_chip.m").read_text()
            self.assertIn("char *poop_apple_chip_name(void)", chip)

    def test_the_uarch_is_unknown_but_apples(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = tree(tmp)
            self.assertEqual(patch(src).returncode, 0)
            uarch = (src / "arm" / "uarch.c").read_text()
            self.assertTrue(uarch.startswith(UARCH), "appended, the rest as it was")
            self.assertIn('fill_uarch(arch, cpu, "Unknown", UARCH_UNKNOWN, CPU_VENDOR_APPLE);', uarch)

    def test_a_source_that_changed_is_refused_and_named(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = tree(tmp, midr=MIDR.replace("return NULL;", "exit(1);"))
            ran = patch(src)
            self.assertNotEqual(ran.returncode, 0)
            self.assertIn("arm/midr.c: expected the text once, found it 0 times", ran.stderr)

    def test_patching_twice_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = tree(tmp)
            self.assertEqual(patch(src).returncode, 0)
            self.assertNotEqual(patch(src).returncode, 0, "the copy is made fresh each build")

    @unittest.skipUnless(SOURCE.is_dir(), "needs target/ios-ports/cpufetch")
    def test_it_applies_to_the_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            copy = Path(tmp) / "src"
            shutil.copytree(SOURCE, copy)
            ran = patch(copy)
            self.assertEqual(ran.returncode, 0, ran.stderr)
            self.assertNotIn("Found invalid cpu_family: 0x%.8X\", cpu_family);\n    return NULL;",
                             (copy / "arm" / "midr.c").read_text())


if __name__ == "__main__":
    unittest.main()
