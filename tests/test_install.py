"""The system-wide installer: layout, desktop entry, icons, terminfo, and uninstall."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1] / 'scripts/install.sh'
SIZES = ['16x16', '32x32', '64x64', '128x128', '256x256', '512x512']


class Install(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.home = self.root / 'home'
        self.home.mkdir()
        self.binary = self.root / 'poopterm'
        self.binary.write_text('#!/bin/sh\n')
        self.stage = self.root / 'stage'
        # ncurses' search path, faked so results do not depend on the host.
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        self.searched('/usr/local/share/terminfo')

    def searched(self, *dirs):
        listing = ''.join(f'{d}\\n' for d in dirs)
        (self.tools / 'infocmp').write_text(f'#!/bin/sh\n[ "$1" = -D ] && printf "{listing}"\n')
        (self.tools / 'infocmp').chmod(0o755)

    def tearDown(self):
        shutil.rmtree(self.root)

    def run_script(self, *args, **env):
        environment = {'PATH': f"{self.tools}:{os.environ['PATH']}", 'HOME': str(self.home), 'POOPTERM_BIN': str(self.binary),
                       'DESTDIR': str(self.stage)}
        environment.update(env)
        result = subprocess.run(['sh', str(SCRIPT), '--no-build', *args], env=environment,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_installs_system_wide_by_default_and_uninstalls_cleanly(self):
        # A per-user data directory no longer moves anything.
        self.run_script(XDG_DATA_HOME=str(self.root / 'data'))
        prefix = self.stage / 'usr/local'
        self.assertTrue(os.access(prefix / 'bin/poopterm', os.X_OK))
        desktop = (prefix / 'share/applications/poopterm.desktop').read_text()
        self.assertIn('\nExec=/usr/local/bin/poopterm\n', desktop)
        self.assertIn('\nTryExec=/usr/local/bin/poopterm\n', desktop)
        self.assertIn('\nStartupWMClass=poopterm\n', desktop)
        for size in SIZES:
            self.assertTrue((prefix / f'share/icons/hicolor/{size}/apps/poopterm.png').is_file(), size)
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(prefix / 'share/applications/poopterm.desktop')],
                           check=True)
        if shutil.which('tic'):
            self.assertTrue(any((prefix / 'share/terminfo').rglob('xterm-poopterm')))
        self.assertFalse((self.root / 'data').exists())
        self.assertFalse((self.home / '.local').exists())

        self.run_script('--uninstall')
        leftovers = [path for path in self.stage.rglob('*poopterm*')]
        self.assertEqual(leftovers, [])

    def fake_tic(self, refuses_full):
        # Writes its source as the compiled entry; like ncurses before 6.1,
        # it can refuse the full entry for its size.
        refusal = ('grep -q "smglr=" "$4" && { echo "error writing $3/x/xterm-poopterm" >&2; exit 1; }'
                   if refuses_full else '')
        (self.tools / 'tic').write_text(
            f'#!/bin/sh\n{refusal}\nmkdir -p "$3/x" && cp "$4" "$3/x/xterm-poopterm"\n')
        (self.tools / 'tic').chmod(0o755)

    def installed_entry(self):
        return (self.stage / 'usr/local/share/terminfo/x/xterm-poopterm').read_text()

    def test_the_full_terminfo_entry_is_installed_where_tic_takes_it(self):
        self.fake_tic(refuses_full=False)
        result = self.run_script()
        entry = self.installed_entry()
        self.assertIn('xterm-poopterm|', entry)
        self.assertIn('\tsmglr=', entry)
        self.assertNotIn('small', result.stdout)

    def test_a_tic_that_refuses_the_full_entry_gets_the_small_one(self):
        # macOS's tic (ncurses 6.0) refuses entries over 4096 bytes.
        self.fake_tic(refuses_full=True)
        result = self.run_script()
        entry = self.installed_entry()
        self.assertIn('xterm-poopterm|', entry)
        self.assertNotIn('\tsmglr=', entry)
        self.assertIn('small terminfo entry', result.stdout)
        # The refusal is explained, not shown as an error.
        self.assertNotIn('error writing', result.stdout + result.stderr)

    @unittest.skipUnless(shutil.which('tic'), 'needs tic')
    def test_terminfo_goes_where_ncurses_looks(self):
        # Fedora's ncurses searches only these, not /usr/local/share/terminfo.
        self.searched(f'{self.home}/.terminfo', '/usr/share/terminfo', '/etc/terminfo')
        self.run_script()
        self.assertTrue(any((self.stage / 'usr/share/terminfo').rglob('xterm-poopterm')))
        self.assertFalse((self.stage / 'usr/local/share/terminfo').exists())
        self.run_script('--uninstall')
        self.assertEqual(list(self.stage.rglob('*poopterm*')), [])
        # A prefix chosen on purpose is kept.
        self.run_script(PREFIX='/opt/poopterm')
        self.assertTrue(any((self.stage / 'opt/poopterm/share/terminfo').rglob('xterm-poopterm')))

    def test_prefix_moves_everything(self):
        self.run_script(PREFIX='/opt/poopterm')
        desktop = (self.stage / 'opt/poopterm/share/applications/poopterm.desktop').read_text()
        self.assertIn('\nExec=/opt/poopterm/bin/poopterm\n', desktop)
        self.assertTrue((self.stage / 'opt/poopterm/bin/poopterm').is_file())

    def test_writable_targets_need_no_root(self):
        # Without DESTDIR the script asks for root only when it cannot write.
        prefix = self.root / 'prefix'
        output = self.run_script(PREFIX=str(prefix), DESTDIR='')
        self.assertTrue((prefix / 'bin/poopterm').is_file())
        self.assertNotIn('sudo', output.stdout + output.stderr)


if __name__ == '__main__':
    unittest.main()
