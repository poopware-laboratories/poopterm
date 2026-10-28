"""Host CPU tuning is local to macOS; cross builds and packages stay portable."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'scripts/build.sh'


class NativeBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.log = self.directory / 'cargo.json'
        self.env = dict(os.environ)
        for name in ('CI', 'DESTDIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                     'CARGO_BUILD_TARGET', 'POOPTERM_PORTABLE'):
            self.env.pop(name, None)
        self.env.update(PATH=f'{self.directory}:{self.env["PATH"]}',
                        BUILD_LOG=str(self.log), TEST_OS='Darwin',
                        TEST_HOST='aarch64-apple-darwin')
        self.tool('uname', '#!/bin/sh\nprintf "%s\\n" "$TEST_OS"\n')
        self.tool('rustc', '#!/bin/sh\nprintf "host: %s\\n" "$TEST_HOST"\n')
        self.tool('cargo', '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
Path(os.environ['BUILD_LOG']).write_text(json.dumps({
    'args': sys.argv[1:],
    'flags': os.environ.get('RUSTFLAGS'),
    'encoded': os.environ.get('CARGO_ENCODED_RUSTFLAGS'),
}))
sys.exit(int(os.environ.get('CARGO_EXIT', '0')))
''')

    def tool(self, name, body):
        path = self.directory / name
        path.write_text(body)
        path.chmod(0o755)

    def build(self, *args, **env):
        result = subprocess.run(['sh', str(SCRIPT), *args],
                                env={**self.env, **env}, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(self.log.read_text())

    def test_local_apple_silicon_build_adds_native_cpu_tuning(self):
        built = self.build('--release', '--locked')
        self.assertEqual(built['args'], ['build', '--release', '--locked'])
        self.assertIn('-Ctarget-cpu=native', built['flags'])
        self.assertIsNone(built['encoded'])

    def test_intel_macos_build_detects_its_own_host(self):
        built = self.build('--release', TEST_HOST='x86_64-apple-darwin')
        self.assertIn('-Ctarget-cpu=native', built['flags'])
        self.assertEqual(built['args'], ['build', '--release'])

    def test_linux_keeps_its_existing_flags_and_cfg_based_musl_policy(self):
        built = self.build('--release', TEST_OS='Linux', RUSTFLAGS='-Cdebuginfo=1')
        self.assertEqual(built['flags'], '-Cdebuginfo=1')
        self.assertEqual(built['args'], ['build', '--release'])

    def test_freebsd_does_not_probe_or_add_a_macos_cpu_flag(self):
        self.tool('rustc', '#!/bin/sh\nexit 89\n')
        built = self.build('--locked', TEST_OS='FreeBSD')
        self.assertIsNone(built['flags'])
        self.assertEqual(built['args'], ['build', '--locked'])

    def test_ci_artifacts_are_portable_even_on_an_apple_silicon_runner(self):
        built = self.build('--release', CI='true')
        self.assertIsNone(built['flags'])
        self.assertIsNone(built['encoded'])

    def test_staged_package_build_is_portable(self):
        built = self.build('--release', DESTDIR='/tmp/package root')
        self.assertIsNone(built['flags'])

    def test_portable_opt_out_is_available_for_local_release_builds(self):
        built = self.build('--release', POOPTERM_PORTABLE='1')
        self.assertIsNone(built['flags'])
        self.assertEqual(built['args'], ['build', '--release'])

    def test_explicit_host_target_can_still_use_native_cpu_tuning(self):
        built = self.build('--release', '--target', 'aarch64-apple-darwin')
        self.assertIn('-Ctarget-cpu=native', built['flags'])
        self.assertEqual(built['args'][-2:], ['--target', 'aarch64-apple-darwin'])

    def test_equal_sign_host_target_keeps_native_cpu_tuning(self):
        built = self.build('--target=aarch64-apple-darwin')
        self.assertIn('-Ctarget-cpu=native', built['flags'])
        self.assertEqual(built['args'][-1], '--target=aarch64-apple-darwin')

    def test_cross_architecture_macos_target_does_not_use_the_host_cpu(self):
        built = self.build('--release', '--target', 'x86_64-apple-darwin')
        self.assertIsNone(built['flags'])
        self.assertEqual(built['args'][-1], 'x86_64-apple-darwin')

    def test_cross_platform_target_does_not_use_the_host_cpu(self):
        built = self.build('--target=x86_64-unknown-freebsd')
        self.assertIsNone(built['flags'])

    def test_multiple_targets_never_apply_one_cpu_to_a_different_architecture(self):
        built = self.build('--target=x86_64-apple-darwin',
                           '--target=aarch64-apple-darwin')
        self.assertIsNone(built['flags'])

    def test_environment_cross_target_is_respected(self):
        built = self.build('--release', CARGO_BUILD_TARGET='aarch64-apple-ios')
        self.assertIsNone(built['flags'])

    def test_command_line_target_takes_precedence_over_environment(self):
        built = self.build('--target=aarch64-apple-darwin',
                           CARGO_BUILD_TARGET='x86_64-apple-darwin')
        self.assertIn('-Ctarget-cpu=native', built['flags'])

    def test_existing_rustflags_are_preserved_including_pgo_paths(self):
        flags = '-Cprofile-use=/tmp/profiles/merged.profdata -Cdebuginfo=1'
        built = self.build('--release', RUSTFLAGS=flags)
        self.assertTrue(built['flags'].startswith(flags))
        self.assertEqual(built['flags'].count('target-cpu=native'), 1)

    def test_explicit_cpu_selection_is_never_overridden(self):
        for flags in ('-Ctarget-cpu=apple-m1', '-C target-cpu=generic',
                      '-Ctarget-cpu=native -Cdebuginfo=1'):
            with self.subTest(flags=flags):
                built = self.build('--release', RUSTFLAGS=flags)
                self.assertEqual(built['flags'], flags)

    def test_encoded_flags_preserve_arguments_with_spaces(self):
        flags = '-Cprofile-use=/tmp/my profiles/a.profdata\x1f-Cdebuginfo=1'
        built = self.build('--release', CARGO_ENCODED_RUSTFLAGS=flags,
                           RUSTFLAGS='ignored by cargo')
        self.assertEqual(built['encoded'].split('\x1f'),
                         [*flags.split('\x1f'), '-Ctarget-cpu=native'])
        self.assertEqual(built['flags'], 'ignored by cargo')

    def test_explicit_encoded_cpu_selection_is_preserved(self):
        flags = '-C\x1ftarget-cpu=apple-m2\x1f-Cdebuginfo=1'
        built = self.build('--release', CARGO_ENCODED_RUSTFLAGS=flags)
        self.assertEqual(built['encoded'], flags)
        self.assertIsNone(built['flags'])

    def test_empty_encoded_flags_do_not_create_an_empty_rustc_argument(self):
        built = self.build('--release', CARGO_ENCODED_RUSTFLAGS='')
        self.assertEqual(built['encoded'], '-Ctarget-cpu=native')
        self.assertIsNone(built['flags'])

    def test_unknown_host_does_not_guess_cpu_compatibility(self):
        self.tool('rustc', '#!/bin/sh\nprintf "rustc without host field\\n"\n')
        built = self.build('--release')
        self.assertIsNone(built['flags'])

    def test_unexpected_cargo_failure_is_propagated_without_retrying(self):
        result = subprocess.run(['sh', str(SCRIPT), '--release'],
                                env={**self.env, 'CARGO_EXIT': '47'},
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 47)
        self.assertTrue(self.log.exists())

    def test_build_arguments_with_spaces_are_forwarded_as_single_arguments(self):
        built = self.build('--target-dir', '/tmp/my build dir', '--release')
        self.assertEqual(built['args'], ['build', '--target-dir', '/tmp/my build dir', '--release'])

    def test_make_pgo_install_respects_a_portable_build_request(self):
        result = subprocess.run(['make', '-n', 'pgo-install', 'POOPTERM_PORTABLE=1'],
                                cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('POOPTERM_PORTABLE=1 sh scripts/pgo.sh dist', result.stdout)

    def test_make_run_builds_with_the_shared_wrapper_before_launching(self):
        result = subprocess.run(['make', '-n', 'run'], cwd=ROOT,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines(),
                         ['sh scripts/build.sh --release --locked', './target/release/poopterm'])


if __name__ == '__main__':
    unittest.main()
