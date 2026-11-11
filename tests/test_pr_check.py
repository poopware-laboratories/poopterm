"""Pull request checks: Conventional Commits titles and the 3:1 ratio of
test lines to production lines."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('pr_check', Path(__file__).parents[1] / 'scripts/pr_check.py')
pr_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pr_check)


class Title(unittest.TestCase):
    def test_conventional_titles_pass(self):
        for title in [
            'feat(render): draw box drawing in code',
            'fix: keep the cursor visible',
            'perf(vt)!: scan escapes with wider vectors',
            'docs(agents): require pull requests',
            'revert: feat(render): draw box drawing in code',
            'chore(deps): bump actions/checkout to v7',
            'build(release): sign the macOS bundle',
            'feat(kitty): support OSC 66 sizes',
        ]:
            self.assertIsNone(pr_check.title_problem(title), title)

    def test_unknown_types_fail(self):
        self.assertIsNotNone(pr_check.title_problem('feature(render): draw boxes'))
        self.assertIsNotNone(pr_check.title_problem('update: draw boxes'))

    def test_a_missing_type_or_colon_fails(self):
        self.assertIsNotNone(pr_check.title_problem('Draw box drawing in code'))
        self.assertIsNotNone(pr_check.title_problem('feat draw boxes'))
        self.assertIsNotNone(pr_check.title_problem('feat(render) draw boxes'))

    def test_summaries_start_lower_case_and_have_no_final_period(self):
        self.assertIsNotNone(pr_check.title_problem('feat(render): Draw boxes'))
        self.assertIsNotNone(pr_check.title_problem('feat(render): draw boxes.'))

    def test_empty_parts_fail(self):
        self.assertIsNotNone(pr_check.title_problem('feat(): draw boxes'))
        self.assertIsNotNone(pr_check.title_problem('feat(render): '))
        self.assertIsNotNone(pr_check.title_problem(''))

    def test_the_problem_names_the_expected_form(self):
        self.assertIn('type(scope): summary', pr_check.title_problem('Draw boxes'))


    def test_scopes_may_hold_paths_and_dots(self):
        self.assertIsNone(pr_check.title_problem('fix(render/boxdraw): close the arc gap'))
        self.assertIsNone(pr_check.title_problem('build(deps.toml): pin the toolchain'))
        self.assertIsNotNone(pr_check.title_problem('fix(Render): close the arc gap'))

    def test_breaking_changes_keep_their_mark(self):
        self.assertIsNone(pr_check.title_problem('feat!: drop the old config keys'))
        self.assertIsNotNone(pr_check.title_problem('feat(!): drop the old config keys'))

    def test_stray_whitespace_fails(self):
        self.assertIsNotNone(pr_check.title_problem('feat:  draw boxes'))
        self.assertIsNotNone(pr_check.title_problem('feat: draw boxes '))
        self.assertIsNotNone(pr_check.title_problem(' feat: draw boxes'))

    def test_summaries_may_hold_any_text(self):
        self.assertIsNone(pr_check.title_problem('fix(render): keep the ➜ of prompts whole'))
        self.assertIsNone(pr_check.title_problem('feat: support OSC 66 (text sizing)'))
        self.assertIsNone(pr_check.title_problem('perf: x'))

class Classify(unittest.TestCase):
    def test_rust_test_files_are_all_tests(self):
        for path in ['src/render/boxdraw/tests.rs', 'src/render/fit_tests.rs', 'src/fonts/font_bench.rs']:
            self.assertEqual(pr_check.test_lines(path, 'fn a() {}\nfn b() {}\n'), [True, True], path)

    def test_rust_sources_are_code_until_their_inline_test_module(self):
        text = 'fn a() {}\n\n#[cfg(test)]\nmod tests {\n\tuse super::*;\n}\n'
        self.assertEqual(pr_check.test_lines('src/a.rs', text), [False, False, True, True, True, True])

    def test_blank_lines_between_attribute_and_module_still_count(self):
        text = 'fn a() {}\n#[cfg(test)]\n\nmod tests {\n}\n'
        self.assertEqual(pr_check.test_lines('src/a.rs', text), [False, True, True, True, True])

    def test_a_test_only_item_or_external_module_is_not_an_inline_module(self):
        text = '#[cfg(test)]\nfn helper() {}\nfn a() {}\n#[cfg(test)]\nmod tests;\nfn b() {}\n'
        self.assertEqual(pr_check.test_lines('src/a.rs', text), [False] * 6)

    def test_python_tests_live_in_the_tests_directory(self):
        self.assertEqual(pr_check.test_lines('tests/test_release.py', 'x = 1\n'), [True])
        self.assertEqual(pr_check.test_lines('scripts/release.py', 'x = 1\n'), [False])

    def test_other_files_are_neither(self):
        for path in ['AGENTS.md', '.github/workflows/ci.yml', 'Cargo.toml', 'scripts/release.sh', 'assets/font.ttf']:
            self.assertIsNone(pr_check.test_lines(path, 'text\n'), path)


    def test_public_inline_modules_are_tests_too(self):
        for module in ['pub mod tests {', 'pub(crate) mod tests {', 'mod kitty_tests {']:
            text = f'fn a() {{}}\n#[cfg(test)]\n{module}\n}}\n'
            self.assertEqual(pr_check.test_lines('src/a.rs', text), [False, True, True, True], module)

    def test_indented_attributes_are_recognized(self):
        text = 'fn a() {}\n    #[cfg(test)]\n    mod tests {\n    }\n'
        self.assertEqual(pr_check.test_lines('src/a.rs', text), [False, True, True, True])

    def test_a_module_named_tests_without_the_attribute_is_code(self):
        self.assertEqual(pr_check.test_lines('src/a.rs', 'mod tests {\n}\n'), [False, False])

    def test_rust_under_a_tests_directory_is_tests(self):
        self.assertEqual(pr_check.test_lines('tests/integration.rs', 'fn a() {}\n'), [True])
        self.assertEqual(pr_check.test_lines('src/term/tests/kitty.rs', 'fn a() {}\n'), [True])

    def test_names_that_merely_contain_test_are_code(self):
        self.assertEqual(pr_check.test_lines('src/attest.rs', 'fn a() {}\n'), [False])
        self.assertEqual(pr_check.test_lines('src/testing.rs', 'fn a() {}\n'), [False])
        self.assertEqual(pr_check.test_lines('scripts/test_helpers.py', 'x = 1\n'), [False])

    def test_empty_files_have_no_lines(self):
        self.assertEqual(pr_check.test_lines('src/a.rs', ''), [])
        self.assertEqual(pr_check.test_lines('tests/test_a.py', ''), [])

class Added(unittest.TestCase):
    DIFF = '\n'.join([
        'diff --git a/src/a.rs b/src/a.rs',
        'index 1..2 100644',
        '--- a/src/a.rs',
        '+++ b/src/a.rs',
        '@@ -3,0 +4,2 @@ fn x() {',
        '+fn y() {}',
        '+fn z() {}',
        '@@ -10 +12 @@',
        '-old',
        '+new',
        'diff --git a/tests/test_b.py b/tests/test_b.py',
        'new file mode 100644',
        '--- /dev/null',
        '+++ b/tests/test_b.py',
        '@@ -0,0 +1,3 @@',
        '+a',
        '+b',
        '+c',
        'diff --git a/gone.rs b/gone.rs',
        'deleted file mode 100644',
        '--- a/gone.rs',
        '+++ /dev/null',
        '@@ -1,2 +0,0 @@',
        '-a',
        '-b',
        '',
    ])

    def test_hunk_headers_give_the_new_line_numbers(self):
        self.assertEqual(pr_check.added_lines(self.DIFF), {'src/a.rs': [4, 5, 12], 'tests/test_b.py': [1, 2, 3]})

    def test_a_count_of_one_may_be_left_out(self):
        self.assertEqual(pr_check.added_lines('+++ b/x.rs\n@@ -1 +7 @@\n'), {'x.rs': [7]})

    def test_pure_deletions_add_nothing(self):
        self.assertEqual(pr_check.added_lines('+++ b/x.rs\n@@ -5,3 +4,0 @@\n'), {})


    def test_binary_files_and_renames_add_no_lines(self):
        diff = '\n'.join([
            'diff --git a/assets/font.ttf b/assets/font.ttf',
            'index 1..2 100644',
            'Binary files a/assets/font.ttf and b/assets/font.ttf differ',
            'diff --git a/src/old.rs b/src/new.rs',
            'similarity index 100%',
            'rename from src/old.rs',
            'rename to src/new.rs',
        ])
        self.assertEqual(pr_check.added_lines(diff), {})

    def test_missing_newline_markers_are_skipped(self):
        diff = '+++ b/x.rs\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n'
        self.assertEqual(pr_check.added_lines(diff), {'x.rs': [1]})

    def test_paths_may_contain_spaces(self):
        self.assertEqual(pr_check.added_lines('+++ b/my dir/x.rs\n@@ -0,0 +1,2 @@\n'), {'my dir/x.rs': [1, 2]})

    def test_hunks_of_one_file_accumulate(self):
        diff = '+++ b/x.rs\n@@ -1,0 +2 @@\n@@ -8,0 +10,2 @@\n+++ b/y.rs\n@@ -0,0 +1 @@\n'
        self.assertEqual(pr_check.added_lines(diff), {'x.rs': [2, 10, 11], 'y.rs': [1]})

class Count(unittest.TestCase):
    FILES = {
        'src/a.rs': 'fn a() {}\n// a comment\n\nfn b() {}\n#[cfg(test)]\nmod tests {\n\t#[test]\n\tfn t() {}\n}\n',
        'tests/test_b.py': 'import x\n# comment\nclass B:\n    pass\n',
        'README.md': 'words\n',
    }

    def count(self, added):
        tests, code, glue, problems = pr_check.count(added, self.FILES.__getitem__)
        self.assertEqual((glue, problems), (0, []), 'no glue markers here')
        return tests, code

    def test_lines_are_split_into_tests_and_code(self):
        self.assertEqual(self.count({'src/a.rs': [1, 4, 6, 7, 8, 9]}), (4, 2))

    def test_blank_and_comment_lines_do_not_count(self):
        self.assertEqual(self.count({'src/a.rs': [2, 3]}), (0, 0))
        self.assertEqual(self.count({'tests/test_b.py': [1, 2, 3, 4]}), (3, 0))

    def test_files_that_are_not_code_do_not_count(self):
        self.assertEqual(self.count({'README.md': [1]}), (0, 0))

    def test_lines_past_the_end_are_ignored(self):
        self.assertEqual(self.count({'src/a.rs': [1, 99]}), (0, 1))


    def test_doc_comments_and_indented_comments_do_not_count(self):
        files = {'src/c.rs': '//! Module docs.\n/// Item docs.\n\t// note\nfn c() {} // trailing\n'}
        self.assertEqual(pr_check.count({'src/c.rs': [1, 2, 3, 4]}, files.__getitem__)[:2], (0, 1))

    def test_python_docstrings_count_as_lines(self):
        files = {'scripts/d.py': '"""Docs."""\n    # note\nx = 1\n'}
        self.assertEqual(pr_check.count({'scripts/d.py': [1, 2, 3]}, files.__getitem__)[:2], (0, 2))

    def test_each_file_is_read_once_and_only_if_it_has_additions(self):
        reads = []
        def read(path):
            reads.append(path)
            return self.FILES[path]
        pr_check.count({'src/a.rs': [1, 4], 'tests/test_b.py': [1]}, read)
        self.assertEqual(sorted(reads), ['src/a.rs', 'tests/test_b.py'])

class Ratio(unittest.TestCase):
    def test_three_test_lines_per_code_line_pass(self):
        self.assertIsNone(pr_check.ratio_problem(30, 10))
        self.assertIsNone(pr_check.ratio_problem(31, 10))

    def test_fewer_fail_and_say_how_many_are_missing(self):
        problem = pr_check.ratio_problem(29, 10)
        self.assertIn('29 test lines for 10 lines of code', problem)
        self.assertIn('1 more', problem)

    def test_changes_without_code_pass(self):
        self.assertIsNone(pr_check.ratio_problem(0, 0))
        self.assertIsNone(pr_check.ratio_problem(5, 0))

    def test_code_without_tests_fails(self):
        self.assertIsNotNone(pr_check.ratio_problem(0, 1))


    def test_the_ratio_scales_to_large_changes(self):
        self.assertIsNone(pr_check.ratio_problem(3000, 1000))
        self.assertIn('add 1 more', pr_check.ratio_problem(2999, 1000))

class Glue(unittest.TestCase):
    """Window glue: marked lines of src/app/ that run only with a real window,
    so no test can reach them. They count for neither side, within limits."""

    def region(self, body, reason='winit calls this when the scale changes'):
        return f'fn a() {{}}\n\t// pr-check: window glue: {reason}\n{body}\t// pr-check: end window glue\nfn b() {{}}\n'

    def check(self, path, text):
        """((tests, code), (glue, problems)) with every line of `text` added."""
        added = {path: list(range(1, len(text.splitlines()) + 1))}
        tests, code, glue, problems = pr_check.count(added, {path: text}.__getitem__)
        return (tests, code), (glue, problems)

    def test_marked_lines_count_for_neither_side_and_are_reported(self):
        text = self.region('\tself.apply_scale();\n\tself.resize(w, h);\n')
        (tests, code), (lines, problems) = self.check('src/app/handler.rs', text)
        self.assertEqual((tests, code), (0, 2), 'only fn a and fn b')
        self.assertEqual((lines, problems), (2, []))

    def test_without_markers_nothing_is_glue(self):
        text = 'fn a() {}\nfn b() {}\n'
        self.assertEqual(self.check('src/app/handler.rs', text), ((0, 2), (0, [])))

    def test_a_region_needs_a_reason(self):
        text = self.region('\tx();\n', reason='')
        (_, code), (lines, problems) = self.check('src/app/a.rs', text)
        self.assertEqual(lines, 0)
        self.assertEqual(code, 3, 'without a reason the lines are code')
        self.assertEqual(len(problems), 1)
        self.assertIn('src/app/a.rs:2', problems[0])
        self.assertIn('reason', problems[0])

    def test_glue_is_only_allowed_in_window_code(self):
        text = self.region('\tx();\n')
        for path in ['src/term/a.rs', 'src/ui/a.rs', 'scripts/a.py', 'src/appendix.rs']:
            (_, code), (lines, problems) = self.check(path, text)
            self.assertEqual(lines, 0, path)
            self.assertIn('allowed only in src/app/', problems[0], path)

    def test_regions_are_short(self):
        body = ''.join(f'\tx{i}();\n' for i in range(pr_check.GLUE_LIMIT))
        self.assertEqual(self.check('src/app/a.rs', self.region(body))[1], (pr_check.GLUE_LIMIT, []))
        longer = body + '\ty();\n'
        (_, code), (lines, problems) = self.check('src/app/a.rs', self.region(longer))
        self.assertEqual(lines, 0, 'an oversized region exempts nothing')
        self.assertEqual(code, pr_check.GLUE_LIMIT + 3)
        self.assertIn(f'at most {pr_check.GLUE_LIMIT}', problems[0])

    def test_blank_and_comment_lines_do_not_use_up_the_limit(self):
        body = ''.join(f'\tx{i}();\n\n\t// why\n' for i in range(pr_check.GLUE_LIMIT))
        self.assertEqual(self.check('src/app/a.rs', self.region(body))[1], (pr_check.GLUE_LIMIT, []))

    def test_an_unterminated_region_is_refused(self):
        text = 'fn a() {}\n// pr-check: window glue: reason\nx();\n'
        (_, code), (lines, problems) = self.check('src/app/a.rs', text)
        self.assertEqual((code, lines), (2, 0))
        self.assertIn('never ends', problems[0])

    def test_regions_do_not_nest_and_an_end_alone_is_refused(self):
        text = '// pr-check: end window glue\nx();\n'
        self.assertIn('without a start', self.check('src/app/a.rs', text)[1][1][0])
        nested = self.region('\t// pr-check: window glue: again\n\tx();\n')
        self.assertIn('inside another', self.check('src/app/a.rs', nested)[1][1][0])

    def test_glue_in_test_code_is_not_needed_and_counts_as_tests(self):
        text = '#[cfg(test)]\nmod tests {\n' + self.region('\tx();\n') + '}\n'
        (tests, code), (lines, _) = self.check('src/app/a.rs', text)
        self.assertEqual((code, lines), (0, 0))
        self.assertEqual(tests, 6, 'the attribute, module, fn a, the call, fn b and the brace')

    def test_only_added_glue_lines_are_counted(self):
        text = self.region('\tx();\n\ty();\n')
        read = {'src/app/a.rs': text}.__getitem__
        self.assertEqual(pr_check.count({'src/app/a.rs': [4]}, read), (0, 0, 1, []))
        self.assertEqual(pr_check.count({'src/app/a.rs': [1]}, read), (0, 1, 0, []))

    def test_markers_in_test_code_are_ignored_even_unbalanced(self):
        text = 'fn a() {}\n#[cfg(test)]\nmod tests {\n\t// pr-check: end window glue\n\tfn t() {}\n}\n'
        (tests, code), (lines, problems) = self.check('src/app/a.rs', text)
        self.assertEqual((tests, code, lines, problems), (4, 1, 0, []))

    def test_a_region_of_only_comments_exempts_nothing_and_is_fine(self):
        text = self.region('\t// a note\n\n')
        self.assertEqual(self.check('src/app/a.rs', text), ((0, 2), (0, [])))

    def test_the_oversized_problem_names_the_count_and_the_start(self):
        body = ''.join(f'\tx{i}();\n' for i in range(pr_check.GLUE_LIMIT + 4))
        problem = self.check('src/app/a.rs', self.region(body))[1][1][0]
        self.assertTrue(problem.startswith('src/app/a.rs:2: window glue'), problem)
        self.assertIn(f'holds {pr_check.GLUE_LIMIT + 4} lines', problem)

    def test_agents_md_states_the_limit_the_check_enforces(self):
        agents = (Path(__file__).parents[1] / 'AGENTS.md').read_text()
        self.assertIn(f'A region holds at most {pr_check.GLUE_LIMIT} lines', agents)
        self.assertIn('// pr-check: window glue: ', agents)
        self.assertIn('// pr-check: end window glue', agents)
        skill = (Path(__file__).parents[1] / '.claude/skills/meeting-the-test-ratio/SKILL.md').read_text()
        self.assertIn(f'at most {pr_check.GLUE_LIMIT} lines', skill)

    def test_directories_inside_the_window_code_are_window_code(self):
        text = self.region('\tx();\n')
        self.assertEqual(self.check('src/app/settings/view.rs', text)[1], (1, []))

    def test_a_file_may_hold_several_regions(self):
        text = self.region('\tx();\n') + self.region('\ty();\n\tz();\n')
        self.assertEqual(self.check('src/app/a.rs', text), ((0, 4), (3, [])))

    def test_markers_may_be_indented_and_the_end_may_have_trailing_blanks(self):
        text = '\t\t// pr-check: window glue: deep in a match\n\t\tx();\n\t\t// pr-check: end window glue  \n'
        self.assertEqual(self.check('src/app/a.rs', text), ((0, 0), (1, [])))

    def test_an_end_marker_with_more_text_does_not_end_the_region(self):
        text = '// pr-check: window glue: reason\nx();\n// pr-check: end window glue here\ny();\n'
        (_, code), (lines, problems) = self.check('src/app/a.rs', text)
        self.assertEqual((code, lines), (2, 0))
        self.assertIn('never ends', problems[0])

    def test_a_reason_of_only_blanks_is_no_reason(self):
        text = self.region('\tx();\n', reason='   ')
        self.assertIn('needs a reason', self.check('src/app/a.rs', text)[1][1][0])

    def test_glue_and_problems_add_up_across_files(self):
        files = {
            'src/app/a.rs': self.region('\tx();\n'),
            'src/app/b.rs': self.region('\ty();\n\tz();\n'),
            'src/c.rs': self.region('\tw();\n'),
        }
        added = {path: list(range(1, 7)) for path in files}
        tests, code, glue, problems = pr_check.count(added, files.__getitem__)
        self.assertEqual((tests, glue), (0, 3))
        self.assertEqual(code, 2 + 2 + 3, 'fn a and fn b in each, and the refused region')
        self.assertEqual([p.split(':')[0] for p in problems], ['src/c.rs'])


class Main(unittest.TestCase):
    def git(self, root, *args):
        subprocess.run(['git', *args], cwd=root, check=True, capture_output=True)

    def repository(self):
        directory = tempfile.TemporaryDirectory()
        root = Path(directory.name)
        self.git(root, 'init', '-q', '-b', 'main')
        self.git(root, 'config', 'user.email', 'ci@example.com')
        self.git(root, 'config', 'user.name', 'CI')
        (root / 'src').mkdir()
        (root / 'src/a.rs').write_text('fn a() {}\n')
        self.git(root, 'add', '.')
        self.git(root, 'commit', '-q', '-m', 'feat: start')
        return directory, root

    def run_check(self, root, title, base='main'):
        base = subprocess.run(['git', 'rev-parse', base], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as errors:
            code = pr_check.main(['--base', base, '--title', title], cwd=root)
        self.errors = errors.getvalue()
        return code

    def test_a_tested_change_with_a_good_title_passes(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/a.rs').write_text('fn a() {}\nfn b() {}\n#[cfg(test)]\nmod tests {\n\tfn t() {}\n}\n')
            self.git(root, 'commit', '-qam', 'feat: add b')
            self.assertEqual(self.run_check(root, 'feat: add b'), 0)

    def test_an_untested_change_fails(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/a.rs').write_text('fn a() {}\nfn b() {}\n')
            self.git(root, 'commit', '-qam', 'feat: add b')
            self.assertEqual(self.run_check(root, 'feat: add b'), 1)

    def test_a_bad_title_fails_even_without_code(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'README.md').write_text('words\n')
            self.git(root, 'add', '.')
            self.git(root, 'commit', '-qm', 'docs: add readme')
            self.assertEqual(self.run_check(root, 'Add readme'), 1)
            self.assertEqual(self.run_check(root, 'docs: add readme'), 0)

    def test_both_problems_are_reported_together(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/a.rs').write_text('fn a() {}\nfn b() {}\n')
            self.git(root, 'commit', '-qam', 'add b')
            self.assertEqual(self.run_check(root, 'Add b'), 1)
            self.assertIn('Conventional Commit', self.errors)
            self.assertIn('3:1', self.errors)

    def test_changes_on_main_after_branching_do_not_count(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'README.md').write_text('words\n')
            self.git(root, 'add', '.')
            self.git(root, 'commit', '-qm', 'docs: add readme')
            self.git(root, 'switch', '-q', 'main')
            (root / 'src/a.rs').write_text('fn a() {}\nfn untested() {}\n')
            self.git(root, 'commit', '-qam', 'feat: untested')
            self.git(root, 'switch', '-q', 'work')
            self.assertEqual(self.run_check(root, 'docs: add readme'), 0)

    def github_merge(self, root):
        """Like a pull_request run: a docs branch, an untested change merged
        to main after it, and HEAD at GitHub's merge of the branch into main.
        Returns the stale base, main before that change."""
        stale = subprocess.run(['git', 'rev-parse', 'main'], cwd=root, check=True, capture_output=True, text=True).stdout.strip()
        self.git(root, 'switch', '-q', '-c', 'work')
        (root / 'README.md').write_text('words\n')
        self.git(root, 'add', '.')
        self.git(root, 'commit', '-qm', 'docs: add readme')
        self.git(root, 'switch', '-q', 'main')
        (root / 'src/a.rs').write_text('fn a() {}\nfn untested() {}\n')
        self.git(root, 'commit', '-qam', 'feat: untested')
        self.git(root, 'switch', '-q', '--detach')
        self.git(root, 'merge', '-q', '--no-ff', '-m', 'Merge work into main', 'work')
        return stale

    def test_on_a_merge_commit_the_first_parent_counts_only_the_pull_request(self):
        directory, root = self.repository()
        with directory:
            self.github_merge(root)
            self.assertEqual(self.run_check(root, 'docs: add readme', base='HEAD^1'), 0)

    def test_a_stale_base_counts_what_main_gained_since(self):
        # Why the workflow does not pass pull_request.base.sha: it can predate
        # merges to main, whose lines would then count for the pull request.
        directory, root = self.repository()
        with directory:
            stale = self.github_merge(root)
            self.assertEqual(self.run_check(root, 'docs: add readme', base=stale), 1)
            self.assertIn('0 test lines for 1 lines of code', self.errors)

    def test_glue_is_reported_and_does_not_need_tests(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/app').mkdir()
            region = '// pr-check: window glue: winit only\nfn on_scale() {}\n// pr-check: end window glue\n'
            (root / 'src/app/h.rs').write_text(region)
            self.git(root, 'add', '.')
            self.git(root, 'commit', '-qm', 'fix: handle scale changes')
            with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
                code = pr_check.main(['--base', 'main', '--title', 'fix: handle scale changes'], cwd=root)
            self.assertEqual(code, 0)
            self.assertEqual(out.getvalue(), '0 test lines, 0 code lines, 1 window glue line\n')
            (root / 'src/app/h.rs').write_text(region.replace('{}\n', '{}\nfn more() {}\n'))
            self.git(root, 'commit', '-qam', 'fix: more')
            with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
                pr_check.main(['--base', 'main', '--title', 'fix: more'], cwd=root)
            self.assertEqual(out.getvalue(), '0 test lines, 0 code lines, 2 window glue lines\n')

    def test_without_glue_the_summary_does_not_mention_it(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/a.rs').write_text('fn a() {}\nfn b() {}\n')
            self.git(root, 'commit', '-qam', 'feat: b')
            with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
                pr_check.main(['--base', 'main', '--title', 'feat: b'], cwd=root)
            self.assertEqual(out.getvalue(), '0 test lines, 1 code lines\n')

    def test_bad_glue_fails_the_check(self):
        directory, root = self.repository()
        with directory:
            self.git(root, 'switch', '-q', '-c', 'work')
            (root / 'src/b.rs').write_text('// pr-check: window glue: no\n// pr-check: end window glue\n')
            self.git(root, 'add', '.')
            self.git(root, 'commit', '-qm', 'fix: b')
            self.assertEqual(self.run_check(root, 'fix: b'), 1)
            self.assertIn('allowed only in src/app/', self.errors)

    def test_the_workflow_compares_with_the_merge_commits_first_parent(self):
        workflow = (Path(__file__).parents[1] / '.github/workflows/pr.yml').read_text()
        self.assertIn('--base HEAD^1', workflow)
        self.assertNotIn('${{ github.event.pull_request.base.sha }}', workflow)


if __name__ == '__main__':
    unittest.main()
