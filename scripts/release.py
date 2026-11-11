#!/usr/bin/env python3
"""Prepare coordinated calendar-version releases; publishing lives in release.sh."""
import datetime
from pathlib import Path
import re
import secrets
import subprocess
import tomllib

# Changelog sections for Conventional Commits types, in order. Other types
# go under Maintenance; release bookkeeping is left out.
SECTIONS = {'feat': 'Added', 'fix': 'Fixed', 'perf': 'Performance'}
BOOKKEEPING = {('chore', 'release'), ('chore', 'homebrew')}
HEADER = re.compile(r'(?P<type>\w+)(?:\((?P<scope>[^)]*)\))?(?P<breaking>!)?: (?P<summary>.+)')
# Releases are weekly. A push releases at once only for a security fix, or
# when the weekly run was missed (lost a race to a merge, or CI was red).
SCHEDULED = {'schedule', 'workflow_dispatch'}
CATCH_UP = datetime.timedelta(days=8)


def parse_header(message):
    return HEADER.fullmatch(message.strip().split('\n', 1)[0].strip())


def security_fix(parsed):
    return parsed is not None and (parsed['type'], parsed['scope']) == ('fix', 'security')


def due(messages, event, last_release, today):
    """Whether a CI run for `event` releases the commits since `last_release`."""
    changes = [parsed for parsed in map(parse_header, messages)
               if not (parsed and (parsed['type'], parsed['scope']) in BOOKKEEPING)]
    if not changes:
        return False
    if event in SCHEDULED:
        return True
    if event != 'push':
        return False
    return (any(map(security_fix, changes)) or last_release is None
            or today - last_release >= CATCH_UP)


def next_version(current, today):
    year, month, patch = map(int, current.split('.'))
    period = (today.year % 100, today.month)
    if period < (year, month):
        raise ValueError('refusing to release a version older than the current month')
    return f'{period[0]}.{period[1]}.{patch + 1 if period == (year, month) else 0}'


def notes(messages):
    """Changelog notes for the commit messages since the last release."""
    sections = {'Breaking changes': [], 'Security': [], **{name: [] for name in SECTIONS.values()}, 'Maintenance': []}
    for message in messages:
        subject = message.strip().split('\n', 1)[0].strip()
        header = HEADER.fullmatch(subject)
        if not header:
            sections['Maintenance'].append(sentence(subject))
            continue
        kind, scope = header['type'], header['scope']
        if (kind, scope) in BOOKKEEPING:
            continue
        entry = sentence(header['summary'], scope)
        # A footer runs until the next footer token or a blank line.
        footer = re.search(r'(?ms)^BREAKING[ -]CHANGE: (.+?)(?=\n\n|\n[\w-]+: |\Z)', message)
        if header['breaking'] or footer:
            explanation = ' '.join(footer[1].split()).rstrip('.') + '.' if footer else ''
            sections['Breaking changes'].append(f'{entry} {explanation}'.rstrip())
        elif security_fix(header):
            sections['Security'].append(sentence(header['summary']))
        else:
            sections[SECTIONS.get(kind, 'Maintenance')].append(entry)
    text = '\n'.join(f'### {name}\n\n' + ''.join(f'- {entry}\n' for entry in entries)
                     for name, entries in sections.items() if entries)
    return text or '- No changes besides release bookkeeping.\n'


def sentence(text, scope=None):
    text = text.strip().rstrip('.')
    text = text[:1].upper() + text[1:]
    return f'{text} ({scope}).' if scope else f'{text}.'


def last_release_tag(root):
    described = subprocess.run(['git', 'describe', '--tags', '--abbrev=0', '--match', 'v*'],
                               cwd=root, capture_output=True, text=True)
    return described.stdout.strip() if described.returncode == 0 else None


def last_release_date(root):
    """The day the newest release tag's commit was made, or None before the first."""
    tag = last_release_tag(root)
    if tag is None:
        return None
    day = subprocess.run(['git', 'log', '-1', '--format=%cs', tag], cwd=root, check=True,
                         capture_output=True, text=True).stdout.strip()
    return datetime.date.fromisoformat(day)


def commits_since_last_release(root):
    """Messages of the commits after the newest release tag, oldest first."""
    tag = last_release_tag(root)
    revisions = f'{tag}..HEAD' if tag else 'HEAD'
    log = subprocess.run(['git', 'log', '--reverse', '--format=%B%x00', revisions],
                         cwd=root, check=True, capture_output=True, text=True).stdout
    return [message for message in log.split('\0') if message.strip()]


def prepare(root, today, messages):
    manifest = (root / 'Cargo.toml').read_text()
    package = tomllib.loads(manifest)['package']
    old = package['version']
    new = next_version(old, today)
    codename = package['metadata']['poopterm']['codename']
    if old.rsplit('.', 1)[0] != new.rsplit('.', 1)[0]:
        previous = codename
        while codename == previous:
            codename = f"{secrets.choice(['Cosmic', 'Disco', 'Turbo', 'Velvet', 'Lunar', 'Electric'])} {secrets.choice(['Beetle', 'Moth', 'Gecko', 'Wombat', 'Otter', 'Fly'])}"
    manifest, count = re.subn(r'(?m)^(version\s*=\s*)"' + re.escape(old) + r'"',
                              lambda m: f'{m[1]}"{new}"', manifest, count=1)
    if count != 1:
        raise ValueError('package version was not found')
    manifest, count = re.subn(r'(?m)^codename\s*=.*$', f'codename = "{codename}"', manifest, count=1)
    if count != 1:
        raise ValueError('codename was not found')
    lock = (root / 'Cargo.lock').read_text()
    lock, count = re.subn(r'(\[\[package\]\]\nname = "poopterm"\nversion = )"' + re.escape(old) + r'"',
                          lambda m: f'{m[1]}"{new}"', lock)
    if count != 1:
        raise ValueError('exactly one poopterm lockfile entry is required')
    changelog = (root / 'CHANGELOG.md').read_text()
    heading, separator, history = changelog.partition('\n')
    entry = f'## {new} "{codename}" — {today.isoformat()}\n\n{notes(messages)}\n'
    (root / 'Cargo.toml').write_text(manifest)
    (root / 'Cargo.lock').write_text(lock)
    (root / 'CHANGELOG.md').write_text(heading + '\n\n' + entry + history.lstrip('\n'))
    return new, codename


def update_formula(root, version):
    path = root / 'Formula/poopterm.rb'
    formula = path.read_text()
    formula, tags = re.subn(r'tag: "v[^"]+"', f'tag: "v{version}"', formula)
    formula, versions = re.subn(r'(?m)^  version "[^"]+"', f'  version "{version}"', formula)
    if tags != 1 or versions != 1:
        raise ValueError('expected one formula tag and version')
    path.write_text(formula)


def update_pkgbuild(root, version):
    path = root / 'packaging/arch/PKGBUILD'
    pkgbuild, versions = re.subn(r'(?m)^pkgver=.*$', f'pkgver={version}', path.read_text())
    pkgbuild, releases = re.subn(r'(?m)^pkgrel=.*$', 'pkgrel=1', pkgbuild)
    if versions != 1 or releases != 1:
        raise ValueError('expected one PKGBUILD pkgver and pkgrel')
    path.write_text(pkgbuild)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--formula', metavar='VERSION')
    parser.add_argument('--pkgbuild', metavar='VERSION')
    parser.add_argument('--due', metavar='EVENT', help='exit 0 if a run for EVENT releases now')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    today = datetime.datetime.now(datetime.timezone.utc).date()
    if args.due is not None:
        raise SystemExit(0 if due(commits_since_last_release(root), args.due,
                                  last_release_date(root), today) else 1)
    if args.formula:
        update_formula(root, args.formula)
    elif args.pkgbuild:
        update_pkgbuild(root, args.pkgbuild)
    else:
        version, codename = prepare(root, today, commits_since_last_release(root))
        print(f'{version}\n{codename}')
