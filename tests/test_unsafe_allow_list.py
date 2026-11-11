"""The allow-list of `unsafe_code` in docs/unsafe.md names exactly the
places in src/ that allow it, so neither can change without the other."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).parents[1]
FILE_WIDE = re.compile(r'^#!\[allow\(unsafe_code\b', re.M)
ON_MOD = re.compile(r'^#\[allow\(unsafe_code\)\]\n(?:#\[[^\n]*\]\n)*(?:pub(?:\([a-z]+\))? )?mod (\w+);', re.M)
# The attribute may be wrapped over several lines.
ON_FN = re.compile(r'#\[allow\(\s*unsafe_code\b[^\]]*\]\n(?:\s*#\[[^\n]*\n)*\s*(?:pub(?:\([a-z]+\))? )?fn (\w+)')


def module_path(path):
    """`src/gpu/mod.rs` → `gpu`, `src/app/gpu.rs` → `app/gpu`, `src/main.rs` → ``."""
    parts = list(path.relative_to(ROOT / 'src').with_suffix('').parts)
    if parts[-1] in ('mod', 'main'):
        parts.pop()
    return '/'.join(parts)


def allowed_in_source():
    """(files, trees, functions) that allow unsafe code: files as paths
    under src/ without `.rs`, trees as `a::b`, functions by name."""
    files, trees, functions = set(), set(), set()
    for path in sorted((ROOT / 'src').rglob('*.rs')):
        text = path.read_text()
        module = module_path(path)
        if FILE_WIDE.search(text):
            files.add(module)
        for name in ON_MOD.findall(text):
            trees.add('::'.join([*module.split('/'), name]) if module else name)
        functions.update(ON_FN.findall(text))
    return files, trees, functions


def allowed_in_docs():
    """The backticked names in the list of the "Where it is allowed" section,
    without the attributes and words it quotes."""
    text = (ROOT / 'docs/unsafe.md').read_text()
    section = text.split('### Where it is allowed', 1)[1]
    bullets = section[section.index('\n- '):].split('\n\n', 1)[0]
    names = set(re.findall(r'`([^`]+)`', bullets))
    return {name for name in names if '(' not in name and name not in ('mod', 'mod.rs')}


class AllowList(unittest.TestCase):
    def setUp(self):
        self.files, self.trees, self.functions = allowed_in_source()
        self.docs = allowed_in_docs()

    def test_every_file_and_tree_that_allows_unsafe_is_listed(self):
        for name in sorted(self.files | self.trees):
            self.assertIn(name, self.docs, f'{name} allows unsafe_code but docs/unsafe.md does not list it')

    def test_every_function_that_allows_unsafe_is_listed(self):
        for name in sorted(self.functions):
            self.assertTrue(
                any(listed.endswith(f'::{name}') for listed in self.docs),
                f'{name} allows unsafe_code but docs/unsafe.md does not list it')

    def test_every_listed_name_allows_unsafe(self):
        known = self.files | self.trees
        for name in sorted(self.docs):
            if '::' in name and name.split('::')[-1] in self.functions:
                continue
            self.assertIn(name, known, f'docs/unsafe.md lists {name}, which does not allow unsafe_code')

    def test_the_lint_is_denied_everywhere_else(self):
        cargo = (ROOT / 'Cargo.toml').read_text()
        self.assertRegex(cargo, r'(?m)^unsafe_code = "deny"$')


class Parsing(unittest.TestCase):
    def test_module_paths_drop_mod_and_main(self):
        self.assertEqual(module_path(ROOT / 'src/gpu/mod.rs'), 'gpu')
        self.assertEqual(module_path(ROOT / 'src/app/gpu.rs'), 'app/gpu')
        self.assertEqual(module_path(ROOT / 'src/main.rs'), '')

    def test_attributes_between_the_allow_and_the_mod_are_skipped(self):
        text = '#[allow(unsafe_code)]\n#[cfg(target_os = "macos")]\npub(crate) mod mtl;\n'
        self.assertEqual(ON_MOD.findall(text), ['mtl'])

    def test_function_allows_may_carry_a_reason_and_other_attributes(self):
        text = '\t#[inline(always)]\n\t#[allow(unsafe_code, reason = "x")]\n\t#[inline]\n\tpub(crate) fn fill(\n'
        self.assertEqual(ON_FN.findall(text), ['fill'])

    def test_function_allows_wrapped_over_lines_are_found(self):
        text = '\t#[allow(\n\t\tunsafe_code,\n\t\treason = "calls the x86-64-v3 copy"\n\t)]\n\tpub(crate) fn advance<H>(\n'
        self.assertEqual(ON_FN.findall(text), ['advance'])

    def test_a_file_wide_allow_is_not_a_tree_or_a_function(self):
        text = '//! A module.\n#![allow(unsafe_code)]\nmod inner;\nfn f() {}\n'
        self.assertTrue(FILE_WIDE.search(text))
        self.assertEqual(ON_MOD.findall(text), [])
        self.assertEqual(ON_FN.findall(text), [])
