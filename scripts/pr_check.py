#!/usr/bin/env python3
"""Checks a pull request: a Conventional Commits title (it becomes the squashed
commit on main, and the changelog line) and at least three added lines of
tests for every added line of production code."""
import argparse
from pathlib import Path
import re
import subprocess
import sys

TYPES = 'feat|fix|perf|refactor|docs|test|bench|build|ci|style|chore|revert'
# The summary starts lower case and has no final period.
TITLE = re.compile(rf'({TYPES})(\([a-z0-9._/-]+\))?!?: (?![A-Z\s])(?!.*[.\s]$).+')
RATIO = 3
HUNK = re.compile(r'@@ -\S+ \+(\d+)(?:,(\d+))? @@')
INLINE_TESTS = re.compile(r'\s*(pub(\([a-z]+\))? )?mod \w+ \{')
# Window glue: a short, marked region of src/app/ that only a real window
# runs (winit events, assigning computed sizes), so no test can reach it.
GLUE_START = re.compile(r'\s*// pr-check: window glue:(.*)')
GLUE_END = re.compile(r'\s*// pr-check: end window glue\s*$')
GLUE_LIMIT = 12


def title_problem(title):
    if TITLE.fullmatch(title):
        return None
    return f'title {title!r} is not a Conventional Commit: type(scope): summary, ' \
        'with a known type, a lower-case summary and no final period'


def test_lines(path, text):
    """Whether each line of a file is test code, or None if it is not code."""
    lines = text.splitlines()
    name = Path(path).name
    if path.endswith('.py'):
        return [path.startswith('tests/')] * len(lines)
    if not path.endswith('.rs'):
        return None
    if name == 'tests.rs' or name.endswith(('_tests.rs', '_bench.rs')) or '/tests/' in f'/{path}':
        return [True] * len(lines)
    # Inline test modules close the file, as docs/testing.md asks.
    for index, line in enumerate(lines):
        if line.strip() == '#[cfg(test)]':
            following = next((rest for rest in lines[index + 1:] if rest.strip()), '')
            if INLINE_TESTS.match(following):
                return [False] * index + [True] * (len(lines) - index)
    return [False] * len(lines)


def glue_regions(path, lines, kinds):
    """The indexes of a file's window glue lines that count for neither side,
    and the problems with its glue markers. Markers in test code are ignored."""
    exempt, problems, start, reason = set(), [], None, ''
    comment = '#' if path.endswith('.py') else '//'
    for index, line in enumerate(lines):
        if kinds[index]:
            continue
        if marker := GLUE_START.match(line):
            if start is not None:
                problems.append(f'{path}:{index + 1}: window glue starts inside another region')
                continue
            start, reason = index, marker[1].strip()
        elif GLUE_END.match(line):
            if start is None:
                problems.append(f'{path}:{index + 1}: window glue ends without a start')
                continue
            body = [i for i in range(start + 1, index)
                    if lines[i].strip() and not lines[i].strip().startswith(comment)]
            where = f'{path}:{start + 1}: window glue'
            if not path.startswith('src/app/'):
                problems.append(f'{where} is allowed only in src/app/, the window code')
            elif not reason:
                problems.append(f'{where} needs a reason after the colon')
            elif len(body) > GLUE_LIMIT:
                problems.append(f'{where} holds {len(body)} lines; at most {GLUE_LIMIT}')
            else:
                exempt.update(body)
            start = None
    if start is not None:
        problems.append(f'{path}:{start + 1}: window glue never ends')
    return exempt, problems


def added_lines(diff):
    """The new line numbers added to each file by a `git diff --unified=0`."""
    added, path = {}, None
    for line in diff.splitlines():
        if line.startswith('+++ '):
            path = line[6:] if line.startswith('+++ b/') else None
        elif (hunk := HUNK.match(line)) and path:
            start, length = int(hunk[1]), int(hunk[2] or 1)
            if length:
                added.setdefault(path, []).extend(range(start, start + length))
    return added


def count(added, read):
    """(test lines, code lines, window glue lines, problems with glue markers)
    among the added lines, skipping blank lines and comments; `read(path)`
    gives a file's new text."""
    tests = code = glue = 0
    problems = []
    for path, numbers in added.items():
        text = read(path)
        kinds = test_lines(path, text)
        if kinds is None:
            continue
        lines = text.splitlines()
        comment = '#' if path.endswith('.py') else '//'
        exempt, file_problems = glue_regions(path, lines, kinds)
        problems += file_problems
        for number in numbers:
            if number > len(lines) or not lines[number - 1].strip() or lines[number - 1].strip().startswith(comment):
                continue
            if number - 1 in exempt:
                glue += 1
            elif kinds[number - 1]:
                tests += 1
            else:
                code += 1
    return tests, code, glue, problems


def ratio_problem(tests, code):
    if tests >= RATIO * code:
        return None
    return f'{tests} test lines for {code} lines of code; the rule is {RATIO}:1, ' \
        f'so add {RATIO * code - tests} more lines of tests'


def main(argv, cwd=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True, help='the commit the pull request is based on')
    parser.add_argument('--title', required=True)
    args = parser.parse_args(argv)
    git = lambda *a: subprocess.run(['git', *a], cwd=cwd, check=True, capture_output=True, text=True).stdout
    diff = git('diff', '--unified=0', '--no-color', f'{args.base}...HEAD')
    root = Path(git('rev-parse', '--show-toplevel').strip())
    tests, code, glue, glue_problems = count(added_lines(diff), lambda path: (root / path).read_text(errors='replace'))
    exempted = f', {glue} window glue line{"" if glue == 1 else "s"}' if glue else ''
    print(f'{tests} test lines, {code} code lines{exempted}')
    problems = [p for p in [title_problem(args.title), ratio_problem(tests, code)] if p] + glue_problems
    for problem in problems:
        print(f'error: {problem}', file=sys.stderr)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
