"""Every file we ship names the repository where it lives now.

The repository moved from poopterm-labs to poopware-laboratories. GitHub
redirects old repository links, but not GitHub Pages, and packages keep
whatever homepage they were built with.
"""
from pathlib import Path
import re
import subprocess
import unittest

ROOT = Path(__file__).parents[1]
HOME = 'https://github.com/poopware-laboratories/poopterm'
OLD = re.compile(r'github\.com/poopterm-labs/|poopterm-labs\.github\.io|--repo poopterm-labs/')


def text(path):
    return (ROOT / path).read_text()


class Urls(unittest.TestCase):
    def test_no_tracked_file_points_at_the_old_owner(self):
        files = subprocess.run(
            ['git', 'ls-files'], cwd=ROOT, capture_output=True, text=True, check=True
        ).stdout.split()
        # The changelog records history; this file names the old owner on purpose.
        skip = {'CHANGELOG.md', 'tests/test_repository_urls.py'}
        stale = []
        for path in files:
            if path in skip or not (ROOT / path).is_file():
                continue
            try:
                lines = text(path).splitlines()
            except UnicodeDecodeError:
                continue
            stale += [f'{path}:{n}' for n, line in enumerate(lines, 1) if OLD.search(line)]
        self.assertEqual(stale, [])

    def test_packages_name_the_repository_as_their_homepage(self):
        deb = re.findall(r'(?m)^Homepage: (.*)$', text('scripts/package-deb.sh'))
        self.assertEqual(deb, [HOME, HOME], 'both Debian packages')
        self.assertEqual(re.findall(r'(?m)^URL: (.*)$', text('scripts/package-rpm.sh')), [HOME])

    def test_the_formula_builds_from_the_repository(self):
        formula = text('Formula/poopterm.rb')
        self.assertIn(f'homepage "{HOME}"', formula)
        self.assertIn(f'url "{HOME}.git",', formula)


if __name__ == '__main__':
    unittest.main()
