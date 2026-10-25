"""The shader build: it compiles each shader with the pinned glslc and
validates it, and with --check fails unless the checked-in SPIR-V is what
the source compiles to, which is what CI relies on."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1] / 'scripts/shaders.sh'

# A glslc that reports a version and "compiles" a source by copying it
# after a marker, and a spirv-val that refuses what says "invalid". Both
# log their arguments.
GLSLC = '''#!/bin/sh
echo "glslc $*" >> "$LOG"
if [ "$1" = --version ]; then echo "shaderc v$VERSION"; exit 0; fi
while [ $# -gt 1 ]; do
\tcase $1 in -o) out=$2; shift 2 ;; *) shift ;; esac
done
grep -q broken "$1" && exit 2
{ echo spirv; cat "$1"; } > "$out"
'''
SPIRV_VAL = '''#!/bin/sh
echo "spirv-val $*" >> "$LOG"
! grep -q invalid "$3"
'''


class Shaders(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        (self.root / 'scripts').mkdir()
        shutil.copy(SCRIPT, self.root / 'scripts/shaders.sh')
        self.shaders = self.root / 'shaders'
        self.shaders.mkdir()
        (self.shaders / 'quad.vert').write_text('vertex\n')
        (self.shaders / 'quad.frag').write_text('fragment\n')
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        for name, body in [('glslc', GLSLC), ('spirv-val', SPIRV_VAL)]:
            (self.tools / name).write_text(body)
            (self.tools / name).chmod(0o755)
        self.temporary = self.root / 'tmp'
        self.temporary.mkdir()
        self.log = self.root / 'log'

    def tearDown(self):
        shutil.rmtree(self.root)

    def run_script(self, *args, version='2026.4'):
        environment = {'PATH': os.environ['PATH'], 'GLSLC': str(self.tools / 'glslc'),
                       'SPIRV_VAL': str(self.tools / 'spirv-val'), 'LOG': str(self.log),
                       'VERSION': version, 'TMPDIR': str(self.temporary)}
        return subprocess.run(['sh', str(self.root / 'scripts/shaders.sh'), *args], env=environment,
                              capture_output=True, text=True)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_each_shader_is_compiled_beside_its_source_and_validated_for_vulkan_1_3(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.shaders / 'quad.vert.spv').read_text(), 'spirv\nvertex\n')
        self.assertEqual((self.shaders / 'quad.frag.spv').read_text(), 'spirv\nfragment\n')
        out = 'shaders'
        self.assertEqual(self.calls()[1:], [
            f'glslc --target-env=vulkan1.3 -O -Werror -o {out}/quad.vert.spv shaders/quad.vert',
            f'spirv-val --target-env vulkan1.3 {out}/quad.vert.spv',
            f'glslc --target-env=vulkan1.3 -O -Werror -o {out}/quad.frag.spv shaders/quad.frag',
            f'spirv-val --target-env vulkan1.3 {out}/quad.frag.spv',
        ])

    def test_the_check_passes_when_the_spirv_is_current_and_leaves_it_alone(self):
        self.run_script()
        compiled = (self.shaders / 'quad.vert.spv').stat().st_mtime_ns
        result = self.run_script('--check')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.shaders / 'quad.vert.spv').stat().st_mtime_ns, compiled)
        # It compiled elsewhere, and cleaned up after itself.
        self.assertNotIn('-o shaders/', ' '.join(self.calls()[6:]))
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_the_check_fails_on_stale_or_missing_spirv_without_touching_it(self):
        self.run_script()
        (self.shaders / 'quad.vert').write_text('changed\n')
        (self.shaders / 'quad.frag.spv').unlink()
        result = self.run_script('--check')
        self.assertEqual(result.returncode, 1)
        self.assertIn('shaders/quad.vert.spv is not what shaders/quad.vert compiles to', result.stderr)
        self.assertIn('shaders/quad.frag.spv is not what shaders/quad.frag compiles to', result.stderr)
        self.assertEqual((self.shaders / 'quad.vert.spv').read_text(), 'spirv\nvertex\n')
        self.assertFalse((self.shaders / 'quad.frag.spv').exists())
        self.assertEqual(list(self.temporary.iterdir()), [])

    def test_another_compiler_version_is_refused_before_compiling(self):
        result = self.run_script(version='2026.3')
        self.assertEqual(result.returncode, 1)
        self.assertIn('glslc is not shaderc 2026.4, of Vulkan SDK 1.4.363.0', result.stderr)
        self.assertEqual(self.calls(), ['glslc --version'])
        self.assertEqual(sorted(path.name for path in self.shaders.iterdir()), ['quad.frag', 'quad.vert'])

    def test_a_shader_that_fails_to_compile_or_validate_fails_the_build(self):
        (self.shaders / 'quad.frag').write_text('broken\n')
        self.assertNotEqual(self.run_script().returncode, 0)
        (self.shaders / 'quad.frag').write_text('invalid\n')
        self.assertNotEqual(self.run_script('--check').returncode, 0)
        self.assertIn(f'spirv-val --target-env vulkan1.3 {self.temporary}', self.calls()[-1])


if __name__ == '__main__':
    unittest.main()
