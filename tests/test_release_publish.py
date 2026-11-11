"""Exercise publication against local Git remotes and a fake GitHub API."""
import datetime
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).parents[1]


class Publish(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        # Like a CI runner, ignore personal Git settings such as tag.gpgsign.
        (self.root / 'gitconfig').write_text('')
        for name, value in [('GIT_CONFIG_GLOBAL', str(self.root / 'gitconfig')), ('GIT_CONFIG_NOSYSTEM', '1')]:
            self.addCleanup(lambda name=name, old=os.environ.get(name): os.environ.pop(name) if old is None else os.environ.__setitem__(name, old))
            os.environ[name] = value
        self.repo = self.root / 'work'
        self.remote = self.root / 'remote.git'
        self.repo.mkdir()
        self.git('init', '--bare', str(self.remote))
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Release test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('remote', 'add', 'origin', str(self.remote))
        (self.repo / 'scripts').mkdir()
        (self.repo / 'Formula').mkdir()
        for name in ['release.py', 'release.sh']:
            shutil.copy(SOURCE / 'scripts' / name, self.repo / 'scripts' / name)
        today = datetime.datetime.now(datetime.timezone.utc).date()
        self.version = f'{today.year % 100}.{today.month}.0'
        (self.repo / 'Cargo.toml').write_text(f'[package]\nname = "poopterm"\nversion = "{self.version}"\n[package.metadata.poopterm]\ncodename = "Giga Fly"\n')
        (self.repo / 'Cargo.lock').write_text(f'version = 4\n[[package]]\nname = "poopterm"\nversion = "{self.version}"\n')
        (self.repo / 'CHANGELOG.md').write_text('# Changelog\n\n## Old\n')
        (self.repo / 'Formula/poopterm.rb').write_text('  tag: "v0.43.0",\n  version "0.43.0"\n')
        (self.repo / 'packaging/arch').mkdir(parents=True)
        (self.repo / 'packaging/arch/PKGBUILD').write_text('pkgname=poopterm\npkgver=0.43.0\npkgrel=2\n')
        # A package left over from before the bump, as CI artifacts used to be.
        dist = self.repo / 'dist'
        dist.mkdir()
        (dist / f'poopterm_{self.version}-1_amd64.deb').write_bytes(b'stale package')
        # Packages whatever version Cargo.toml holds, or FAKE_DEB_VERSION.
        package = self.repo / 'scripts/package-deb.sh'
        package.write_text('''#!/usr/bin/env bash
set -euo pipefail
version=${FAKE_DEB_VERSION:-$(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)}
rm -f dist/poopterm_*.deb
echo "package $version" >> "$FAKE_GH_ROOT/build.log"
echo built > "dist/poopterm_${version}-1_amd64.deb"
''')
        package.chmod(0o755)
        rpm = self.repo / 'scripts/package-rpm.sh'
        rpm.write_text('''#!/usr/bin/env bash
set -euo pipefail
version=${FAKE_RPM_VERSION:-$(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)}
rm -f dist/poopterm-*.rpm
echo "rpm $version" >> "$FAKE_GH_ROOT/build.log"
echo built > "dist/poopterm-${version}-1.x86_64.rpm"
''')
        rpm.chmod(0o755)
        self.git('add', '.')
        self.git('commit', '-m', 'Initial checked revision')
        self.sha = self.git('rev-parse', 'HEAD').strip()
        self.git('push', 'origin', 'main')
        fakebin = self.root / 'bin'
        fakebin.mkdir()
        gh = fakebin / 'gh'
        gh.write_text('''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
root = Path(os.environ['FAKE_GH_ROOT'])
with (root / 'api.log').open('a') as log:
    log.write(' '.join(sys.argv[1:]) + '\\n')
if (root / 'fail-api').exists():
    sys.stderr.write('HTTP 503 unavailable\\n')
    sys.exit(1)
# `released` holds the release's tag; `draft` marks it unpublished, and
# `immutable` makes published releases refuse new assets, as GitHub does.
tag = (root / 'released').read_text().strip() if (root / 'released').exists() else None
draft = (root / 'draft').exists()
if sys.argv[1] == 'api':
    assets = (root / 'assets').read_text().split() if (root / 'assets').exists() else []
    releases = [{'tag_name': 'v0.1.0', 'draft': True, 'assets': [{'name': 'poopterm_0.1.0-1_amd64.deb'}]}]
    if tag:
        releases.insert(0, {'tag_name': tag, 'draft': draft, 'assets': [{'name': name} for name in assets]})
    print(json.dumps(releases))
if sys.argv[1:3] == ['release', 'create']:
    (root / 'released').write_text(sys.argv[3])
    if '--draft' in sys.argv:
        (root / 'draft').touch()
if sys.argv[1:3] == ['release', 'upload']:
    if (root / 'fail-upload').exists():
        sys.exit(1)
    if (root / 'immutable').exists() and not draft:
        sys.stderr.write('HTTP 422: Cannot upload assets to an immutable release.\\n')
        sys.exit(1)
    with (root / 'assets').open('a') as assets:
        assets.write(Path(sys.argv[4]).name + '\\n')
if sys.argv[1:3] == ['release', 'edit'] and '--draft=false' in sys.argv:
    (root / 'draft').unlink(missing_ok=True)
''')
        gh.chmod(0o755)
        # Releases ship the PGO build; pgo.sh itself is tested in test_pgo.
        pgo = self.repo / 'scripts/pgo.sh'
        pgo.write_text('''#!/bin/sh
echo "pgo $* $(sed -n 's/^version = "\\(.*\\)"/\\1/p' Cargo.toml)" >> "$FAKE_GH_ROOT/build.log"
[ ! -e "$FAKE_GH_ROOT/fail-build" ]
''')
        self.env = dict(os.environ, GITHUB_SHA=self.sha, GITHUB_EVENT_NAME='schedule', GH_REPO='test/poopterm',
                        GH_TOKEN='fake-local-test-token', FAKE_GH_ROOT=str(self.root),
                        PATH=str(fakebin) + os.pathsep + os.environ['PATH'])

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, stderr=subprocess.PIPE, text=True)

    def publish(self):
        return subprocess.run(['bash', 'scripts/release.sh'], cwd=self.repo, env=self.env,
                              capture_output=True, text=True)

    def test_release_tags_bump_then_updates_formula_and_reruns_are_idempotent(self):
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertIn(f'version = "{version}"', self.git('show', f'v{version}:Cargo.toml'))
        self.assertIn(f'tag: "v{version}"', self.git('show', 'origin/main:Formula/poopterm.rb'))
        rerun = self.publish()
        self.assertEqual(rerun.returncode, 0)
        self.assertIn('already published', rerun.stdout)
        self.assertEqual((self.root / 'api.log').read_text().count('release create'), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release upload'), 2)
        self.assertEqual(len((self.root / 'build.log').read_text().splitlines()), 3)

    def test_a_release_without_its_package_gets_it_on_a_rerun(self):
        # Published without its package, as before releases started as
        # drafts: the rerun builds and uploads only.
        (self.root / 'released').write_text(f'v{self.next_version()}')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        log = (self.root / 'api.log').read_text()
        self.assertEqual(log.count('release create'), 0)
        self.assertEqual(log.count('release upload'), 2)

    def test_release_commits_do_not_start_another_ci_run(self):
        # The deploy key's pushes trigger workflows, unlike GITHUB_TOKEN's;
        # without the marker each release commit would release again.
        self.assertEqual(self.publish().returncode, 0)
        message = self.git('log', '-1', '--format=%B', 'origin/main')
        self.assertIn('[skip ci]', message, message)
        self.assertTrue(message.startswith('chore(release): '), message)

    def test_a_release_pushes_to_main_once_with_the_formula_in_the_tagged_commit(self):
        # A second push, after publishing, raced every merge made meanwhile.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.git('rev-parse', 'origin/main~1').strip(), self.sha)
        self.assertEqual(self.git('rev-parse', 'origin/main').strip(),
                         self.git('rev-parse', f'v{version}^{{commit}}').strip())
        formula = self.git('show', f'v{version}:Formula/poopterm.rb')
        self.assertIn(f'tag: "v{version}"', formula)
        self.assertIn(f'version "{version}"', formula)
        changed = self.git('show', '--name-only', '--format=', 'origin/main').split()
        self.assertEqual(sorted(changed), ['CHANGELOG.md', 'Cargo.lock', 'Cargo.toml', 'Formula/poopterm.rb',
                                           'packaging/arch/PKGBUILD'])

    def test_the_tagged_commit_points_the_pkgbuild_at_its_own_tag(self):
        # makepkg builds tag v$pkgver, so the tag must name itself.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        pkgbuild = self.git('show', f'v{self.next_version()}:packaging/arch/PKGBUILD')
        self.assertEqual(pkgbuild, f'pkgname=poopterm\npkgver={self.next_version()}\npkgrel=1\n')

    def test_a_broken_pkgbuild_stops_the_release_before_any_push(self):
        (self.repo / 'packaging/arch/PKGBUILD').write_text('pkgname=poopterm\n')
        self.git('commit', '-qam', 'Lose pkgver')
        self.git('push', '-q', 'origin', 'main')
        self.env['GITHUB_SHA'] = self.git('rev-parse', 'HEAD').strip()
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('expected one PKGBUILD pkgver', result.stderr)
        self.assertEqual(self.git('ls-remote', '--tags', str(self.remote)), '')

    def test_a_rerun_after_a_publishing_failure_pushes_nothing_more(self):
        failure = self.root / 'fail-api'
        failure.touch()
        self.assertNotEqual(self.publish().returncode, 0)
        pushed = self.git('ls-remote', str(self.remote))
        failure.unlink()
        self.assertEqual(self.publish().returncode, 0)
        self.assertEqual(self.git('ls-remote', str(self.remote)), pushed)

    def test_a_push_refused_for_another_reason_still_fails(self):
        hook = self.remote / 'hooks/pre-receive'
        hook.write_text('#!/bin/sh\necho refused by policy >&2\nexit 1\n')
        hook.chmod(0o755)
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('main moved', result.stdout)
        self.assertFalse((self.root / 'api.log').exists())

    def commit_on_main(self, subject, date=None):
        """Commits and pushes `subject` to main as CI would check it, dated `date` days ago."""
        env = dict(os.environ)
        if date is not None:
            when = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=date)
            env['GIT_COMMITTER_DATE'] = env['GIT_AUTHOR_DATE'] = when.isoformat()
        subprocess.check_output(['git', 'commit', '-q', '--allow-empty', '-m', subject], cwd=self.repo, env=env)
        self.git('push', '-q', 'origin', 'main')
        self.env['GITHUB_SHA'] = self.git('rev-parse', 'HEAD').strip()

    def released_after(self, days, subject):
        """A release `days` ago, then `subject` pushed today."""
        self.commit_on_main('chore(release): earlier release', date=days)
        self.git('tag', f'v{self.version}')
        self.git('push', '-q', 'origin', f'v{self.version}')
        self.commit_on_main(subject)

    def test_a_week_without_changes_releases_nothing(self):
        self.assertEqual(self.publish().returncode, 0)
        tags = self.git('ls-remote', '--tags', str(self.remote))
        self.env['GITHUB_SHA'] = self.git('rev-parse', 'origin/main').strip()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('nothing to release', result.stdout)
        self.assertEqual(self.git('ls-remote', '--tags', str(self.remote)), tags)

    def test_a_push_does_not_release_before_the_week_is_up(self):
        self.released_after(3, 'feat: add a thing')
        self.env['GITHUB_EVENT_NAME'] = 'push'
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('not due', result.stdout)
        self.assertEqual(self.git('rev-parse', 'origin/main').strip(), self.env['GITHUB_SHA'])
        self.assertFalse((self.root / 'api.log').exists())

    def test_a_push_releases_a_security_fix_at_once(self):
        self.released_after(1, 'fix(security): bound OSC 52 payloads')
        self.env['GITHUB_EVENT_NAME'] = 'push'
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        notes = self.git('show', f'v{self.next_version()}:CHANGELOG.md')
        self.assertIn('### Security\n\n- Bound OSC 52 payloads.\n', notes)

    def test_a_push_catches_up_when_the_weekly_run_was_missed(self):
        self.released_after(10, 'feat: add a thing')
        self.env['GITHUB_EVENT_NAME'] = 'push'
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f'v{self.next_version()}', self.git('ls-remote', '--tags', str(self.remote)))

    def test_a_run_without_its_event_fails_before_any_push(self):
        del self.env['GITHUB_EVENT_NAME']
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertEqual(self.git('ls-remote', '--tags', str(self.remote)), '')

    def test_an_empty_event_fails_before_any_push(self):
        self.env['GITHUB_EVENT_NAME'] = ''
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertEqual(self.git('ls-remote', '--tags', str(self.remote)), '')

    def test_asking_with_an_empty_event_never_releases(self):
        due = subprocess.run(['python3', 'scripts/release.py', '--due', ''], cwd=self.repo,
                             capture_output=True, text=True)
        self.assertEqual(due.returncode, 1, due.stdout + due.stderr)
        self.assertEqual(self.git('status', '--porcelain', '--untracked-files=no'), '')

    def next_version(self):
        return self.version.rsplit('.', 1)[0] + '.1'

    def calls(self):
        log = (self.root / 'api.log').read_text().splitlines()
        return [' '.join(line.split()[:2]) for line in log if line.startswith('release ')]

    def test_a_release_is_published_only_after_its_package_is_attached(self):
        # Immutable releases refuse assets once published.
        (self.root / 'immutable').touch()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), ['release create', 'release upload', 'release upload', 'release edit'])
        log = (self.root / 'api.log').read_text()
        self.assertIn(f'release create v{self.next_version()} --draft', log)
        self.assertIn(f'release edit v{self.next_version()} --draft=false', log)
        self.assertFalse((self.root / 'draft').exists())

    def test_a_draft_left_without_its_package_is_finished_on_a_rerun(self):
        (self.root / 'immutable').touch()
        (self.root / 'fail-upload').touch()
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertTrue((self.root / 'draft').exists(), 'the failed upload leaves a draft')
        self.assertNotIn('release edit', (self.root / 'api.log').read_text())
        (self.root / 'fail-upload').unlink()
        (self.root / 'api.log').unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls(), ['release upload', 'release upload', 'release edit'])
        self.assertEqual(self.uploads(), self.packages(self.next_version()))
        self.assertFalse((self.root / 'draft').exists())

    def test_a_draft_holding_its_package_is_only_published(self):
        self.assertEqual(self.publish().returncode, 0)
        (self.root / 'draft').touch()
        (self.root / 'api.log').unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('already published', result.stdout)
        self.assertEqual(self.calls(), ['release edit'])
        self.assertFalse((self.root / 'draft').exists())

    def test_drafts_of_other_versions_are_ignored(self):
        # The fake API always lists a v0.1.0 draft holding its package.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('v0.1.0', (self.root / 'api.log').read_text().split('\n', 1)[1])
        self.assertEqual(self.calls(), ['release create', 'release upload', 'release upload', 'release edit'])

    def uploads(self):
        log = (self.root / 'api.log').read_text().splitlines()
        return [line.split()[-2] for line in log if line.startswith('release upload')]

    def test_the_package_is_built_from_the_bumped_release_commit(self):
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), self.packages(version))
        build = (self.root / 'build.log').read_text().splitlines()
        self.assertEqual(build, [f'pgo dist {version}', f'package {version}', f'rpm {version}'])

    def packages(self, version):
        return [f'dist/poopterm_{version}-1_amd64.deb', f'dist/poopterm-{version}-1.x86_64.rpm']

    def test_a_release_holding_only_its_debian_package_gets_the_rpm(self):
        # Releases before RPMs, or a run that failed between the two uploads.
        (self.root / 'released').write_text(f'v{self.next_version()}')
        (self.root / 'draft').touch()
        (self.root / 'assets').write_text(f'poopterm_{self.next_version()}-1_amd64.deb\n')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.uploads(), [f'dist/poopterm-{self.next_version()}-1.x86_64.rpm'])
        self.assertEqual(self.calls(), ['release upload', 'release edit'])

    def test_a_release_holding_only_its_rpm_gets_the_debian_package(self):
        (self.root / 'released').write_text(f'v{self.next_version()}')
        (self.root / 'assets').write_text(f'poopterm-{self.next_version()}-1.x86_64.rpm\n')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn('already published', result.stdout)
        self.assertEqual(self.uploads(), [f'dist/poopterm_{self.next_version()}-1_amd64.deb'])

    def test_a_published_release_needs_both_packages_to_count_as_finished(self):
        (self.root / 'released').write_text(f'v{self.next_version()}')
        (self.root / 'assets').write_text(f'poopterm_{self.next_version()}-1_amd64.deb\n'
                                          f'poopterm-{self.next_version()}-1.x86_64.rpm\n')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('already published', result.stdout)
        self.assertEqual(self.uploads(), [])
        self.assertFalse((self.root / 'build.log').exists())

    def test_another_versions_rpm_is_never_uploaded(self):
        self.env['FAKE_RPM_VERSION'] = self.version
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('one RPM package', result.stderr)
        self.assertEqual(self.uploads(), [])
        self.assertNotIn('release create', (self.root / 'api.log').read_text())

    def test_a_failed_rpm_build_stops_before_the_github_release(self):
        (self.repo / 'scripts/package-rpm.sh').write_text('#!/bin/sh\nexit 1\n')
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('release create', (self.root / 'api.log').read_text())

    def test_a_package_of_another_version_is_never_uploaded(self):
        self.env['FAKE_DEB_VERSION'] = self.version
        result = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('expected exactly one', result.stderr)
        self.assertEqual(self.uploads(), [])

    def test_a_failed_build_stops_before_the_github_release(self):
        (self.root / 'fail-build').touch()
        self.assertNotEqual(self.publish().returncode, 0)
        self.assertNotIn('release create', (self.root / 'api.log').read_text())
        (self.root / 'fail-build').unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), self.packages(version))
        self.assertEqual(len(self.git('tag').splitlines()), 1)

    def test_moved_main_skips_every_release_mutation(self):
        (self.repo / 'new').touch()
        self.git('add', 'new')
        self.git('commit', '-m', 'New unchecked revision')
        self.git('push', 'origin', 'main')
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('main moved', result.stdout)
        self.assertEqual(self.git('tag'), '')
        self.assertFalse((self.root / 'api.log').exists())
        self.assertFalse((self.root / 'build.log').exists())

    def test_api_failure_stops_and_rerun_resumes_without_another_bump(self):
        failure = self.root / 'fail-api'
        failure.touch()
        self.assertNotEqual(self.publish().returncode, 0)
        failure.unlink()
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(self.git('tag').splitlines()), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release create'), 1)
        self.assertEqual((self.root / 'api.log').read_text().count('release upload'), 2)
        version = self.version.rsplit('.', 1)[0] + '.1'
        self.assertEqual(self.uploads(), self.packages(version))

    def test_a_push_race_cannot_publish_a_tag_or_overwrite_main(self):
        racer = self.root / 'racer'
        self.git('clone', '--branch', 'main', str(self.remote), str(racer))
        real_git = shutil.which('git')
        for args in [('config', 'user.name', 'Racer'), ('config', 'user.email', 'race@example.invalid')]:
            subprocess.check_call([real_git, '-C', str(racer), *args], stdout=subprocess.DEVNULL)
        (racer / 'racing-change').touch()
        subprocess.check_call([real_git, '-C', str(racer), 'add', '.'])
        subprocess.check_call([real_git, '-C', str(racer), 'commit', '-m', 'Race'], stdout=subprocess.DEVNULL)
        race_sha = subprocess.check_output([real_git, '-C', str(racer), 'rev-parse', 'HEAD'], text=True).strip()
        wrapper = self.root / 'bin/git'
        wrapper.write_text(f'''#!/usr/bin/env python3
import os
import subprocess
import sys
if '--atomic' in sys.argv:
    subprocess.check_call([{real_git!r}, '-C', {str(racer)!r}, 'push', 'origin', 'HEAD:main'])
os.execv({real_git!r}, [{real_git!r}, *sys.argv[1:]])
''')
        wrapper.chmod(0o755)
        # Losing the race is a skip, like a main that moved before the run:
        # the racing commit's own run releases it.
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('main moved', result.stdout)
        actual = subprocess.check_output([real_git, '--git-dir', str(self.remote), 'rev-parse', 'main'], text=True).strip()
        self.assertEqual(actual, race_sha)
        self.assertEqual(subprocess.check_output([real_git, '--git-dir', str(self.remote), 'tag'], text=True), '')
        self.assertFalse((self.root / 'api.log').exists())
        self.assertFalse((self.root / 'build.log').exists())


if __name__ == '__main__':
    unittest.main()
