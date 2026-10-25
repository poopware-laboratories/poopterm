"""Pin the parts of the CI workflow that other steps and people rely on."""
from pathlib import Path
import re
import unittest

WORKFLOW = (Path(__file__).parents[1] / '.github/workflows/ci.yml').read_text()


def step(name):
    """The lines of the workflow step called `name`, up to the next step or job."""
    lines = WORKFLOW.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.strip() == f'- name: {name}':
            indent = len(line) - len(line.lstrip())
            body = []
            for rest in lines[index + 1:]:
                if rest.strip() and len(rest) - len(rest.lstrip()) <= indent:
                    break
                body.append(rest)
            return ''.join(body)
    raise AssertionError(f'no step {name!r}')


class DebianPackage(unittest.TestCase):
    def test_the_package_uploads_as_the_deb_itself_not_a_zip(self):
        # People trying a pull request's package download and install it as is.
        upload = step('Upload Debian package')
        self.assertIn('actions/upload-artifact@v7', upload)
        self.assertRegex(upload, r'archive: false\n')
        self.assertRegex(upload, r'path: dist/\*\.deb\n')

    def test_the_direct_upload_has_no_name_to_download_it_by(self):
        # A direct upload is named after its file, so a step downloading
        # `poopterm-deb` would fail; releases build their own package instead.
        self.assertNotIn('name: poopterm-deb', WORKFLOW)
        self.assertNotIn('download-artifact', WORKFLOW)

    def test_a_missing_package_fails_the_job(self):
        self.assertRegex(step('Upload Debian package'), r'if-no-files-found: error\n')

    def test_the_step_lookup_rejects_unknown_steps(self):
        with self.assertRaises(AssertionError):
            step('No such step')

    def test_the_step_lookup_stops_at_the_next_step_or_job(self):
        self.assertNotIn('run:', step('Install nightly Rust').split('run:', 1)[1])
        self.assertNotIn('release:', step('Upload Debian package'))


def job(name):
    """The lines of the workflow job called `name`, up to the next job."""
    match = WORKFLOW.split(f'\n  {name}:\n', 1)
    if len(match) != 2:
        raise AssertionError(f'no job {name!r}')
    body = []
    for line in match[1].splitlines(keepends=True):
        if line.strip() and len(line) - len(line.lstrip()) <= 2:
            break
        body.append(line)
    return ''.join(body)


class Release(unittest.TestCase):
    def test_the_release_job_installs_a_python_with_tomllib(self):
        # Ubuntu 22.04, kept for the package's glibc floor, ships Python 3.10;
        # the release scripts read Cargo.toml with tomllib, new in 3.11.
        release = job('release')
        self.assertIn('runs-on: ubuntu-22.04', release)
        setup = step('Install Python for the release scripts')
        self.assertIn('actions/setup-python@v7', setup)
        version = re.search(r"python-version: '3\.(\d+)'", setup)
        self.assertIsNotNone(version, setup)
        self.assertGreaterEqual(int(version[1]), 11)

    def test_python_is_installed_before_the_release_scripts_run(self):
        release = job('release')
        self.assertIn('- name: Install Python for the release scripts', release)
        self.assertLess(release.index('- name: Install Python for the release scripts'),
                        release.index('- name: Publish the checked main revision'))

    def test_the_release_scripts_need_tomllib(self):
        # If they stop needing it, the setup step above can go.
        scripts = Path(__file__).parents[1] / 'scripts'
        self.assertIn('import tomllib', (scripts / 'release.py').read_text())
        self.assertIn('import tomllib', (scripts / 'release.sh').read_text())

    def test_releases_have_the_tools_and_time_for_a_pgo_build(self):
        # pgo.sh merges profiles with the toolchain's own llvm-profdata, and
        # builds three times: instrumented, then trained, then optimized.
        release = job('release')
        self.assertIn('run: rustup toolchain install nightly --profile minimal --component llvm-tools\n',
                      release)
        self.assertGreaterEqual(int(re.search(r'timeout-minutes: (\d+)', release)[1]), 60)
        self.assertIn('bash scripts/pgo.sh dist', (Path(__file__).parents[1] / 'scripts/release.sh').read_text())

    def test_the_job_lookup_stops_at_the_next_job(self):
        self.assertNotIn('release:', job('debian-package'))
        self.assertIn('scripts/release.sh', job('release'))
        with self.assertRaises(AssertionError):
            job('no-such-job')


class Provenance(unittest.TestCase):
    def test_released_packages_get_a_signed_build_provenance(self):
        # `gh attestation verify` then proves which workflow run and commit
        # built a downloaded package.
        attest = step('Attest the released package')
        self.assertIn('uses: actions/attest-build-provenance@v4', attest)
        self.assertRegex(attest, r'subject-path: \|\n +dist/poopterm_\*\.deb\n +dist/poopterm-\*\.rpm\n')

    def test_provenance_is_attested_after_the_release_built_the_package(self):
        release = job('release')
        self.assertLess(release.index('- name: Publish the checked main revision'),
                        release.index('- name: Attest the released package'))

    def test_runs_that_skip_the_release_attest_nothing(self):
        # A run that lost the race to a newer commit, or found the release
        # finished, builds no package.
        attest = step('Attest the released package')
        self.assertIn("if: hashFiles('dist/poopterm_*.deb') != ''", attest)

    def test_only_the_release_job_may_sign(self):
        release = job('release')
        for permission in ['contents: write', 'id-token: write', 'attestations: write']:
            self.assertIn(permission, release)
        top = WORKFLOW.split('\njobs:', 1)[0]
        self.assertIn('contents: read', top)
        self.assertNotIn('id-token', top)
        self.assertEqual(WORKFLOW.count('id-token: write'), 1)


class FedoraPackage(unittest.TestCase):
    def test_the_rpm_is_built_installed_and_run_on_fedora(self):
        fedora = job('fedora-package')
        self.assertIn('container: registry.fedoraproject.org/fedora:latest', fedora)
        self.assertIn('cargo build --release --locked', fedora)
        install = step('Install the RPM package and run it')
        self.assertLess(fedora.index('bash scripts/package-rpm.sh'), fedora.index('dnf install -y dist/*.rpm'))
        for check in ['rpm -V poopterm', 'poopterm --version', 'infocmp xterm-poopterm']:
            self.assertIn(check, install)

    def test_the_packaging_script_tests_run_where_rpmbuild_exists(self):
        # They skip on the Ubuntu and macOS runners, which lack rpmbuild.
        setup = step('Install Fedora packaging libraries and nightly Rust')
        for package in ['rpm-build', 'ncurses', 'cpio', 'python3']:
            self.assertIn(f' {package} ', setup)
        self.assertIn('python3 -m unittest tests.test_package_rpm', step('Test the RPM packaging script'))

    def test_the_package_uploads_as_the_rpm_itself_not_a_zip(self):
        upload = step('Upload RPM package')
        self.assertRegex(upload, r'archive: false\n')
        self.assertRegex(upload, r'path: dist/\*\.rpm\n')
        self.assertRegex(upload, r'if-no-files-found: error\n')

    def test_releases_wait_for_the_fedora_package(self):
        self.assertRegex(job('release'), r'needs: \[[^]]*fedora-package[^]]*\]')

    def test_the_release_job_can_build_rpms_on_ubuntu(self):
        # release.sh packages the RPM from the same PGO binary as the .deb.
        self.assertRegex(step('Install Linux, Debian and RPM packaging libraries'), r' rpm\n')
        self.assertIn('bash scripts/package-rpm.sh', (Path(__file__).parents[1] / 'scripts/release.sh').read_text())
class ArchPackage(unittest.TestCase):
    def test_git_is_installed_before_checkout_so_the_repository_is_cloned(self):
        # Without git, checkout downloads a tarball and makepkg has nothing to clone.
        arch = job('arch-package')
        self.assertIn('container: archlinux:latest', arch)
        self.assertIn(' git ', step('Install Arch packaging tools'))
        self.assertLess(arch.index('Install Arch packaging tools'), arch.index('actions/checkout@v7'))
        self.assertIn('fetch-depth: 0', arch)

    def test_pull_requests_build_their_own_code_not_the_released_tag(self):
        build = step('Build the Arch package from this checkout')
        self.assertIn('git branch -f arch-ci HEAD', build)
        self.assertIn('git+file://$GITHUB_WORKSPACE#branch=arch-ci', build)
        self.assertIn("su builder -c 'makepkg --noconfirm'", build)

    def test_the_package_is_installed_and_run_without_its_debug_package(self):
        install = step('Install the Arch package and run it')
        self.assertIn('pacman -U --noconfirm /tmp/arch/poopterm-[0-9]*.pkg.tar.zst', install)
        self.assertIn('poopterm --version', install)
        self.assertIn('infocmp xterm-poopterm', install)

    def test_releases_wait_for_the_arch_package(self):
        self.assertRegex(job('release'), r'needs: \[[^]]*arch-package[^]]*\]')


class FreeBSD(unittest.TestCase):
    """The FreeBSD VM is the slowest job; it runs only what needs FreeBSD."""

    def vm(self):
        return step('Build and test on FreeBSD')

    def test_the_vm_runs_the_tests_and_the_binary(self):
        vm = self.vm()
        self.assertIn('cargo test --locked', vm)
        self.assertIn('cargo build --locked', vm)
        self.assertIn('./target/debug/poopterm --version', vm)

    def test_the_vm_skips_the_slow_release_build(self):
        # Fat LTO took half the job, and no FreeBSD binary ships.
        self.assertNotIn('--release', self.vm())

    def test_lints_and_formatting_run_on_linux_instead(self):
        vm = self.vm()
        self.assertNotIn('cargo fmt', vm)
        self.assertNotIn('cargo clippy', vm)
        self.assertNotIn('--component clippy', vm)
        bsd = step('Check BSD-specific code')
        self.assertIn('cargo +nightly clippy --locked --target x86_64-unknown-freebsd --all-targets -- -D warnings', bsd)

    def test_the_vm_installs_libraries_not_the_whole_x_server(self):
        prepare = self.vm().split('prepare:', 1)[1].split('run:', 1)[0]
        packages = prepare.split('pkg install -y', 1)[1].split('\n', 1)[0].split()
        self.assertNotIn('xorg', packages)
        for package in ['pkgconf', 'libX11', 'libxkbcommon', 'wayland', 'bash', 'zsh']:
            self.assertIn(package, packages)


class Speed(unittest.TestCase):
    def test_the_linux_check_leaves_release_builds_to_the_package_job(self):
        check = job('check')
        build = step('Build release binary')
        self.assertIn("if: runner.os == 'macOS'", build)
        self.assertIn('cargo +nightly build --release --locked', job('debian-package'))
        self.assertIn("if: runner.os == 'macOS'", step('Verify binary execution'))
        self.assertIn('Build release binary', check)

    def test_a_new_push_to_a_pull_request_cancels_its_older_run(self):
        self.assertIn("concurrency:\n  group: ci-${{ github.event_name == 'pull_request' && github.ref || github.sha }}\n", WORKFLOW)
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", WORKFLOW)

    def test_runs_on_main_are_never_cancelled(self):
        # Cancelling one could stop a release halfway through publishing; a
        # group per commit also keeps pushes to main from queueing (a group
        # holds one running and one pending run, and drops older pending ones).
        self.assertNotIn('cancel-in-progress: true', WORKFLOW)
        self.assertIn('|| github.sha }}', WORKFLOW)
        release = job('release')
        self.assertIn('group: poopterm-release', release)
        self.assertIn('cancel-in-progress: false', release)


SHADERS_SCRIPT = (Path(__file__).parents[1] / 'scripts/shaders.sh').read_text()


class Shaders(unittest.TestCase):
    def sdk(self):
        return re.search(r'^SDK=(\S+)$', SHADERS_SCRIPT, re.M)[1]

    def test_the_linux_check_compares_the_spirv_with_its_source(self):
        check = job('check')
        self.assertIn('- name: Check the SPIR-V matches the shaders', check)
        compare = step('Check the SPIR-V matches the shaders')
        self.assertIn("if: runner.os == 'Linux'", compare)
        self.assertIn('scripts/shaders.sh --check', compare)
        self.assertIn('GLSLC=~/vulkan-sdk/glslc SPIRV_VAL=~/vulkan-sdk/spirv-val', compare)
        # The compiler is installed before it is used.
        self.assertLess(check.index('- name: Install the shader compiler'),
                        check.index('- name: Check the SPIR-V matches the shaders'))

    def test_ci_uses_the_sdk_the_script_names(self):
        # Another glslc compiles to other bytes, and the check would fail.
        sdk = self.sdk()
        install = step('Install the shader compiler')
        self.assertIn(f'/sdk/download/{sdk}/linux/vulkansdk-linux-x86_64-{sdk}.tar.xz', install)
        self.assertIn(f'{sdk}/x86_64/bin/glslc {sdk}/x86_64/bin/spirv-val', install)
        self.assertIn(f'key: vulkan-sdk-{sdk}-', step('Cache the shader compiler'))
        self.assertEqual(WORKFLOW.count('1.4.'), 5, 'every SDK version is the one checked here')

    def test_the_download_is_checked_before_it_is_unpacked(self):
        install = step('Install the shader compiler')
        checksum = re.search(r'echo "([0-9a-f]{64})  /tmp/sdk\.tar\.xz" \| sha256sum -c', install)
        self.assertIsNotNone(checksum, install)
        self.assertLess(install.index('sha256sum -c'), install.index('tar -xJf'))
        # Only on Linux, where it runs, and when the cache missed.
        self.assertIn("if: runner.os == 'Linux' && steps.shader-tools.outputs.cache-hit != 'true'", install)
        self.assertIn("if: runner.os == 'Linux'", step('Cache the shader compiler'))
        self.assertIn('id: shader-tools', step('Cache the shader compiler'))

    def test_the_script_compiles_for_vulkan_1_3_with_warnings_as_errors_and_validates(self):
        self.assertIn('--target-env=vulkan1.3 -O -Werror', SHADERS_SCRIPT)
        self.assertIn('"$SPIRV_VAL" --target-env vulkan1.3', SHADERS_SCRIPT)
        self.assertIn('cmp -s', SHADERS_SCRIPT)
        self.assertTrue(SHADERS_SCRIPT.startswith('#!/bin/sh\n'))
        self.assertIn('set -eu', SHADERS_SCRIPT)


if __name__ == '__main__':
    unittest.main()
