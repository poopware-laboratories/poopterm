"""The Arch Linux PKGBUILD: it builds the tagged release, and releases bump it."""
from pathlib import Path
import importlib.util
import re
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).parents[1]
PKGBUILD = (ROOT / 'packaging/arch/PKGBUILD').read_text()
spec = importlib.util.spec_from_file_location('release', ROOT / 'scripts/release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def field(name):
    match = re.search(rf'(?m)^{name}=(.*)$', PKGBUILD)
    if match is None:
        raise AssertionError(f'no {name} in the PKGBUILD')
    return match[1]


class Contents(unittest.TestCase):
    def test_pkgver_matches_the_crate_version(self):
        # The release commit bumps both, so the tag it builds is the one it names.
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(field('pkgver'), cargo['package']['version'])
        self.assertEqual(field('pkgrel'), '1')

    def test_it_builds_the_tag_of_its_own_version(self):
        self.assertEqual(field('source'), '("$pkgname::git+$url.git#tag=v$pkgver")')
        self.assertEqual(field('url'), "'https://github.com/poopware-laboratories/poopterm'")

    def test_the_license_matches_the_crate(self):
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(field('license'), f"('{cargo['package']['license']}')")

    def test_it_installs_through_install_sh_under_usr(self):
        # Desktop entry, icons and terminfo stay in one place for every package.
        self.assertIn('DESTDIR="$pkgdir" PREFIX=/usr POOPTERM_BIN=target/release/poopterm sh scripts/install.sh --no-build',
                      PKGBUILD)
        self.assertIn('/usr/share/licenses/$pkgname/LICENSE', PKGBUILD)

    def test_the_build_uses_the_locked_dependencies_offline(self):
        self.assertIn('cargo fetch --locked', PKGBUILD)
        self.assertIn('cargo build --release --frozen', PKGBUILD)

    def test_makepkg_lto_is_off_so_mimalloc_links(self):
        # makepkg's lto adds GCC's -flto to CFLAGS: mimalloc's C objects become
        # GCC bitcode that rust-lld cannot read (undefined mi_malloc_aligned).
        # Cargo's own LTO, from the release profile, still applies.
        self.assertIn("'!lto'", field('options'))

    def test_the_toolchain_stays_out_of_the_builders_home(self):
        self.assertEqual(PKGBUILD.count('export RUSTUP_HOME="$srcdir/rustup" CARGO_HOME="$srcdir/cargo"'), 2)

    def test_the_display_libraries_winit_loads_are_declared(self):
        # winit dlopens them, so nothing would pull them in otherwise.
        self.assertIn("'libxkbcommon'", field('depends'))
        for library in ['wayland', 'libx11', 'libxkbcommon-x11']:
            self.assertIn(f"'{library}: ", PKGBUILD)

    def test_the_lookup_rejects_missing_fields(self):
        with self.assertRaises(AssertionError):
            field('epoch')


class Bump(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / 'packaging/arch').mkdir(parents=True)
        self.path = self.root / 'packaging/arch/PKGBUILD'

    def test_a_release_sets_pkgver_and_resets_pkgrel(self):
        self.path.write_text('pkgname=poopterm\npkgver=26.10.59\npkgrel=3\nsource=("v$pkgver")\n')
        release.update_pkgbuild(self.root, '26.11.0')
        self.assertEqual(self.path.read_text(), 'pkgname=poopterm\npkgver=26.11.0\npkgrel=1\nsource=("v$pkgver")\n')

    def test_only_whole_assignment_lines_change(self):
        # A function body mentioning pkgver keeps its text.
        body = 'pkgver=1.0\npkgrel=1\ncheck() {\n\techo "$pkgver"\n\t_pkgver=2\n}\n'
        self.path.write_text(body)
        release.update_pkgbuild(self.root, '26.11.0')
        self.assertEqual(self.path.read_text(), body.replace('pkgver=1.0', 'pkgver=26.11.0', 1))

    def test_a_pkgbuild_without_exactly_one_pkgver_is_refused(self):
        for text in ['pkgrel=1\n', 'pkgver=1\npkgver=2\npkgrel=1\n', 'pkgver=1\n']:
            self.path.write_text(text)
            with self.assertRaises(ValueError):
                release.update_pkgbuild(self.root, '26.11.0')
            self.assertEqual(self.path.read_text(), text)


if __name__ == '__main__':
    unittest.main()
