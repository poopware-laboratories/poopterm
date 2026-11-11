"""Release versioning, codename retention, and coordinated file updates."""
import datetime
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).parents[1] / 'scripts/release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class Release(unittest.TestCase):
    def test_same_month_increments_patch_and_keeps_codename(self):
        self.assertEqual(release.next_version('26.10.7', datetime.date(2026, 10, 18)), '26.10.8')

    def test_new_month_and_year_start_at_zero(self):
        self.assertEqual(release.next_version('26.9.4', datetime.date(2026, 10, 1)), '26.10.0')
        self.assertEqual(release.next_version('26.12.9', datetime.date(2027, 1, 1)), '27.1.0')
        with self.assertRaises(ValueError):
            release.next_version('27.1.0', datetime.date(2026, 12, 31))

    def test_plan_updates_only_package_versions_and_prepends_notes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[package]\nname = "poopterm"\nversion = "26.10.0"\n[package.metadata.poopterm]\ncodename = "Giga Fly"\n[dependencies]\nfoo = "26.10.0"\n')
            (root / 'Cargo.lock').write_text('version = 4\n\n[[package]]\nname = "foo"\nversion = "26.10.0"\n\n[[package]]\nname = "poopterm"\nversion = "26.10.0"\n')
            (root / 'CHANGELOG.md').write_text('# Changelog\n\n## Older\n')
            result = release.prepare(root, datetime.date(2026, 10, 18), ['feat(parser): support OSC 66'])
            self.assertEqual(result, ('26.10.1', 'Giga Fly'))
            self.assertIn('foo = "26.10.0"', (root / 'Cargo.toml').read_text())
            self.assertIn('name = "foo"\nversion = "26.10.0"', (root / 'Cargo.lock').read_text())
            self.assertIn('name = "poopterm"\nversion = "26.10.1"', (root / 'Cargo.lock').read_text())
            self.assertTrue((root / 'CHANGELOG.md').read_text().startswith(
                '# Changelog\n\n## 26.10.1 "Giga Fly" — 2026-10-18\n\n### Added\n\n- Support OSC 66 (parser).\n\n## Older\n'))
            result = release.prepare(root, datetime.date(2026, 11, 1), [])
            self.assertEqual(result[0], '26.11.0')
            self.assertNotEqual(result[1], 'Giga Fly')


    def test_notes_group_conventional_commits_by_type(self):
        commits = [
            'feat(parser): support kitty\'s text sizing protocol\n\nWhy it matters.\n',
            'fix: keep images in the scrollback',
            'perf(term): freeze a frame for synchronized output',
            'docs: describe the new escape sequences',
            'chore(release): 26.10.3 after green CI\n\nPoopterm-Release-For: abc\n',
            'chore(homebrew): point formula at published v26.10.3',
            'build(install)!: install system-wide only\n\nBody.\n\nBREAKING CHANGE: scripts/install.sh needs\nroot.\nCo-Authored-By: Someone <a@b>\n',
            'Merge something without a type',
        ]
        self.assertEqual(release.notes(commits), '''### Breaking changes

- Install system-wide only (install). scripts/install.sh needs root.

### Added

- Support kitty's text sizing protocol (parser).

### Fixed

- Keep images in the scrollback.

### Performance

- Freeze a frame for synchronized output (term).

### Maintenance

- Describe the new escape sequences.
- Merge something without a type.
''')

    def test_notes_without_changes_say_so(self):
        self.assertEqual(release.notes(['chore(release): 26.10.3 after green CI']),
                         '- No changes besides release bookkeeping.\n')

    def test_security_fixes_come_right_after_breaking_changes(self):
        commits = ['feat: add a thing', 'fix(security): bound OSC 52 payloads',
                   'fix(security)!: refuse unsigned updates\n\nBREAKING CHANGE: Sign them.']
        self.assertEqual(release.notes(commits), '''### Breaking changes

- Refuse unsigned updates (security). Sign them.

### Security

- Bound OSC 52 payloads.

### Added

- Add a thing.
''')


MONDAY = datetime.date(2026, 11, 16)
WEEK_AGO = MONDAY - datetime.timedelta(days=7)


class Due(unittest.TestCase):
    """Releases are weekly; a security fix or a missed week releases on a push."""

    def due(self, messages, event, last=WEEK_AGO, today=MONDAY):
        return release.due(messages, event, last, today)

    def test_the_weekly_schedule_releases_what_changed(self):
        self.assertTrue(self.due(['fix: keep images'], 'schedule'))

    def test_the_schedule_skips_a_week_without_changes(self):
        self.assertFalse(self.due([], 'schedule'))
        self.assertFalse(self.due(['chore(release): 26.11.3 after green CI\n\n[skip ci]'], 'schedule'))

    def test_a_manual_run_releases_at_once(self):
        self.assertTrue(self.due(['docs: fix a typo'], 'workflow_dispatch', last=MONDAY))
        self.assertFalse(self.due([], 'workflow_dispatch'))

    def test_pushes_wait_for_the_schedule(self):
        self.assertFalse(self.due(['feat: add a thing'], 'push', last=MONDAY))
        self.assertFalse(self.due(['feat: add a thing'], 'push'))

    def test_a_push_catches_up_a_missed_week_after_eight_days(self):
        # Seven days is the schedule's own turn; one more means it was missed.
        last = MONDAY - datetime.timedelta(days=8)
        self.assertTrue(self.due(['feat: add a thing'], 'push', last=last))
        self.assertFalse(self.due(['feat: add a thing'], 'push', last=last + datetime.timedelta(days=1)))

    def test_a_push_releases_the_first_version(self):
        self.assertTrue(self.due(['feat: add a thing'], 'push', last=None))

    def test_a_security_fix_releases_on_its_push(self):
        for fix in ['fix(security): bound OSC 52 payloads', 'fix(security)!: refuse it\n\nBREAKING CHANGE: x.']:
            self.assertTrue(self.due(['docs: a', fix], 'push', last=MONDAY), fix)

    def test_only_fixes_scoped_security_are_urgent(self):
        for other in ['feat(security): add a sandbox', 'fix(osc): mention security', 'fix: security of OSC 52']:
            self.assertFalse(self.due([other], 'push', last=MONDAY), other)

    def test_an_unknown_event_never_releases(self):
        # A pull request's run must not publish whatever it built.
        self.assertFalse(self.due(['fix(security): x'], 'pull_request', last=None))


if __name__ == '__main__':
    unittest.main()
