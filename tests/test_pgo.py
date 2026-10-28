"""Profile-guided and BOLT builds: training runs, the shipped binary, and the fallbacks."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).parents[1]
HOST = 'x86_64-unknown-test'

# Builds a stand-in poopterm: instrumented builds log their runs and write a
# profile, optimized ones name the profile they used, plain ones say so.
CARGO = '''#!/bin/sh
echo "cargo $*" >> "$FAKE/log"
echo "env strip=${CARGO_PROFILE_RELEASE_STRIP-unset} $RUSTFLAGS" >> "$FAKE/log"
# Like cargo, refuse an empty profile override.
[ "${CARGO_PROFILE_RELEASE_STRIP-unset}" = '' ] && exit 1
[ -e "$FAKE/fail-build" ] && exit 1
dir=target/release
while [ $# -gt 0 ]; do
	case $1 in
	--target-dir) dir=$2/%s/release; shift ;;
	esac
	shift
done
mkdir -p "$dir"
case $RUSTFLAGS in
*-Cprofile-generate=*)
	profiles=${RUSTFLAGS##*-Cprofile-generate=}
	profiles=${profiles%% *}
	cat > "$dir/poopterm" <<EOF
#!/bin/sh
echo "train \\$*" >> "$FAKE/log"
[ -e "$FAKE/fail-train" ] && exit 1
touch "$profiles/run\\$\\$.profraw"
EOF
	;;
*-Cprofile-use=*) printf '#!/bin/sh\\necho optimized with %%s\\n' "${RUSTFLAGS##*-Cprofile-use=}" > "$dir/poopterm" ;;
*) printf '#!/bin/sh\\necho plain\\n' > "$dir/poopterm" ;;
esac
chmod +x "$dir/poopterm"
''' % HOST

RUSTC = f'''#!/bin/sh
case $1 in
-vV) echo "host: {HOST}" ;;
--print) echo "$FAKE/sysroot" ;;
esac
'''

PROFDATA = '''#!/bin/sh
echo "profdata $1 $2 $(($# - 3))" >> "$FAKE/log"
touch "$3"
'''


# Instrumenting writes a binary that logs its runs and leaves a profile per
# process; optimizing writes one that names the profile it was laid out with.
BOLT = r'''#!/bin/sh
echo "llvm-bolt $*" >> "$FAKE/log"
[ -e "$FAKE/fail-bolt" ] && exit 1
shift
profile=
while [ $# -gt 0 ]; do
	case $1 in
	-o) out=$2; shift ;;
	-data=*) data=${1#-data=} ;;
	-instrumentation-file=*) profile=${1#*=} ;;
	esac
	shift
done
if [ -n "$profile" ]; then
	printf '#!/bin/sh\necho "bolt-train $*" >> "$FAKE/log"\ntouch "%s.$$.fdata"\n' "$profile" > "$out"
else
	printf '#!/bin/sh\necho bolted with %s\n' "$data" > "$out"
fi
chmod +x "$out"
'''

MERGE_FDATA = '''#!/bin/sh
echo "merge-fdata $#" >> "$FAKE/log"
'''

STRIP = '''#!/bin/sh
echo "strip $*" >> "$FAKE/log"
[ "$1" = -o ] && cp "$3" "$2"
exit 0
'''

UNAME = '''#!/bin/sh
echo "${FAKE_OS:-Linux}"
'''

LAYOUT = ('-reorder-blocks=ext-tsp -reorder-functions=cdsort -split-functions -split-all-cold '
          '-split-eh -icf -use-gnu-stack -no-huge-pages -use-old-text')


class Pgo(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.fake = Path(self.temp.name).resolve()  # macOS: /var is /private/var
        self.repo = self.fake / 'repo'
        (self.repo / 'scripts').mkdir(parents=True)
        shutil.copy(SOURCE / 'scripts/pgo.sh', self.repo / 'scripts/pgo.sh')
        shutil.copy(SOURCE / 'scripts/build.sh', self.repo / 'scripts/build.sh')
        bin_dir = self.fake / 'bin'
        bin_dir.mkdir()
        tools = self.fake / f'sysroot/lib/rustlib/{HOST}/bin'
        tools.mkdir(parents=True)
        for path, text in [(bin_dir / 'cargo', CARGO), (bin_dir / 'rustc', RUSTC),
                           (tools / 'llvm-profdata', PROFDATA), (bin_dir / 'llvm-bolt', BOLT),
                           (bin_dir / 'merge-fdata', MERGE_FDATA), (bin_dir / 'strip', STRIP),
                           (bin_dir / 'uname', UNAME)]:
            path.write_text(text)
            path.chmod(0o755)
        self.env = dict(os.environ, FAKE=str(self.fake),
                        PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}")

    def pgo(self, *args, **env):
        return subprocess.run(['sh', 'scripts/pgo.sh', *args], cwd=self.repo,
                              env=dict(self.env, **env),
                              capture_output=True, text=True)

    def log(self):
        return (self.fake / 'log').read_text().splitlines()

    def optimized_flags(self):
        return [line for line in self.log() if 'profile-use' in line]

    def shipped(self):
        return subprocess.check_output([self.repo / 'target/release/poopterm'], text=True).strip()

    def test_training_covers_floods_and_small_interactive_writes(self):
        result = self.pgo('train', '2')
        self.assertEqual(result.returncode, 0, result.stderr)
        runs = [line for line in self.log() if line.startswith('train')]
        self.assertEqual(runs, ['train --benchmark --mib 2',
                                'train --benchmark --kib 64 --read 16 --frame-every 64'])

    def test_dist_ships_the_optimized_build_trained_on_every_run(self):
        result = self.pgo('dist', FAKE_OS='Darwin')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('shipping the PGO build', result.stdout)
        self.assertEqual(self.shipped(), f'optimized with {self.repo}/target/pgo/merged.profdata')
        # Both training runs left a profile, and both were merged.
        self.assertIn('profdata merge -o 2', self.log())
        self.assertNotIn('cargo build --release --locked', self.log())

    def test_dist_passes_the_training_size_on(self):
        self.assertEqual(self.pgo('dist', '3').returncode, 0)
        self.assertIn('train --benchmark --mib 3', self.log())

    def test_failed_training_ships_a_plain_build_and_says_so(self):
        (self.fake / 'fail-train').touch()
        result = self.pgo('dist')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::warning::pgo.sh: PGO failed; shipping a plain release build', result.stderr)
        self.assertEqual(self.shipped(), 'plain')
        self.assertEqual(self.log()[-2], 'cargo build --release --locked')

    def test_a_failed_plain_build_fails_dist(self):
        (self.fake / 'fail-build').touch()
        result = self.pgo('dist')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.repo / 'target/release/poopterm').exists())

    def test_dist_works_from_any_directory(self):
        result = subprocess.run(['sh', './pgo.sh', 'dist'], cwd=self.repo / 'scripts', env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('bolted with', self.shipped())

    def test_linux_pgo_builds_keep_relocations_and_symbols_for_bolt(self):
        self.assertEqual(self.pgo('all').returncode, 0)
        [flags] = self.optimized_flags()
        self.assertTrue(flags.startswith('env strip=false '), flags)
        self.assertIn('-Clink-arg=-Wl,--emit-relocs', flags)

    def test_other_systems_build_pgo_as_a_plain_release_does(self):
        self.assertEqual(self.pgo('all', FAKE_OS='Darwin').returncode, 0)
        [flags] = self.optimized_flags()
        self.assertTrue(flags.startswith('env strip=unset '), flags)
        self.assertNotIn('emit-relocs', flags)

    def test_bolt_profiles_the_pgo_build_and_lays_it_out_again(self):
        self.assertEqual(self.pgo('all').returncode, 0)
        result = self.pgo('bolt', '2')
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log()
        start = log.index(next(line for line in log if line.startswith('llvm-bolt')))
        bolt = f'{self.repo}/target/pgo/bolt'
        optimized = f'{self.repo}/target/pgo/optimized/{HOST}/release/poopterm'
        self.assertEqual(log[start:], [
            f'llvm-bolt {optimized} -o {bolt}/instrumented -instrument '
            f'-instrumentation-file={bolt}/profile -instrumentation-file-append-pid',
            'bolt-train --benchmark --mib 2',
            'bolt-train --benchmark --kib 64 --read 16 --frame-every 64',
            'merge-fdata 2',
            f'llvm-bolt {optimized} -o {bolt}/poopterm -data={bolt}/merged.fdata {LAYOUT}',
            f'strip {bolt}/poopterm',
        ])
        self.assertIn('Bolted build:', result.stdout)

    def test_bolt_forgets_the_profiles_of_earlier_runs(self):
        self.assertEqual(self.pgo('all').returncode, 0)
        self.assertEqual(self.pgo('bolt').returncode, 0)
        self.assertEqual(self.pgo('bolt').returncode, 0)
        self.assertEqual([line for line in self.log() if line.startswith('merge-fdata')],
                         ['merge-fdata 2', 'merge-fdata 2'])

    def test_bolt_builds_the_pgo_binary_when_there_is_none(self):
        self.assertEqual(self.pgo('train').returncode, 0)
        result = self.pgo('bolt')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.optimized_flags()), 1)

    def test_dist_ships_the_bolted_pgo_build_on_linux(self):
        result = self.pgo('dist')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('shipping the PGO+BOLT build', result.stdout)
        self.assertEqual(self.shipped(), f'bolted with {self.repo}/target/pgo/bolt/merged.fdata')

    def test_failed_bolt_ships_the_stripped_pgo_build_and_says_so(self):
        (self.fake / 'fail-bolt').touch()
        result = self.pgo('dist')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::warning::pgo.sh: BOLT failed; shipping the PGO build', result.stderr)
        self.assertEqual(self.shipped(), f'optimized with {self.repo}/target/pgo/merged.profdata')
        self.assertEqual(self.log()[-1], f'strip -o {self.repo}/target/release/poopterm '
                                         f'{self.repo}/target/pgo/optimized/{HOST}/release/poopterm')

    def test_dist_without_bolt_ships_the_stripped_pgo_build(self):
        result = self.pgo('dist', LLVM_BOLT='llvm-bolt-missing')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('::notice::pgo.sh: no llvm-bolt-missing; shipping the PGO build', result.stderr)
        self.assertTrue(self.shipped().startswith('optimized with'))
        self.assertFalse([line for line in self.log() if line.startswith('llvm-bolt')])
        self.assertTrue(self.log()[-1].startswith('strip -o '))

    def test_dist_needs_merge_fdata_for_bolt_too(self):
        result = self.pgo('dist', MERGE_FDATA='merge-fdata-missing')
        self.assertIn('::notice::pgo.sh: no merge-fdata-missing', result.stderr)
        self.assertTrue(self.shipped().startswith('optimized with'))

    def test_bolt_tools_can_carry_a_version_suffix(self):
        # Debian and Ubuntu install them as llvm-bolt-18 and merge-fdata-18.
        bin_dir = self.fake / 'bin'
        for tool in ['llvm-bolt', 'merge-fdata']:
            (bin_dir / tool).rename(bin_dir / f'{tool}-18')
        result = self.pgo('dist', LLVM_BOLT='llvm-bolt-18', MERGE_FDATA='merge-fdata-18')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.shipped().startswith('bolted with'))

    def test_clean_forgets_pgo_and_bolt_profiles(self):
        self.assertEqual(self.pgo('dist').returncode, 0)
        self.assertEqual(self.pgo('clean').returncode, 0)
        pgo = self.repo / 'target/pgo'
        self.assertEqual(sorted(path.name for path in pgo.iterdir()),
                         ['instrumented', 'optimized'])

    def test_help_prints_the_whole_header_and_no_code(self):
        result = self.pgo('--help')
        self.assertEqual(result.returncode, 0)
        lines = result.stdout.splitlines()
        self.assertTrue(lines[0].startswith('Profile-guided optimization'), lines[0])
        self.assertTrue(lines[-1].startswith('suffix, as Debian'), lines[-1])
        self.assertIn('  scripts/pgo.sh bolt [MIB]', '\n'.join(lines))

    def test_make_installs_what_releases_ship(self):
        dry_run = subprocess.check_output(['make', '-n', 'pgo-install', 'PGO_MIB=3'], cwd=SOURCE,
                                          text=True).splitlines()
        self.assertEqual(dry_run[0], 'POOPTERM_PORTABLE=0 sh scripts/pgo.sh dist 3')
        self.assertIn('sh scripts/install.sh --no-build', dry_run[1])
        self.assertNotIn('POOPTERM_BIN', dry_run[1])

    def test_make_bolt_passes_the_training_size_on(self):
        dry_run = subprocess.check_output(['make', '-n', 'bolt', 'PGO_MIB=3'], cwd=SOURCE, text=True)
        self.assertEqual(dry_run.strip(), 'sh scripts/pgo.sh bolt 3')

    def test_dist_on_other_systems_ships_pgo_without_bolt(self):
        result = self.pgo('dist', FAKE_OS='Darwin')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('notice', result.stderr)
        self.assertFalse([line for line in self.log() if line.startswith(('llvm-bolt', 'strip'))])


if __name__ == '__main__':
    unittest.main()
