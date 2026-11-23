#!/bin/sh
# Builds command libraries for Poopterm on iOS from Apple's open-source
# command sources (shell_cmds, file_cmds, text_cmds), cloned into
# target/ios-ports/. Each command becomes a dynamic library exporting
# `poopterm_entry` (see poopterm_cmd.c), in target/ios-commands/<sdk>/, named
# as the command (no extension) so the shell finds it on $PATH.
#
#   ios/commands/build.sh [iphonesimulator|iphoneos|macosx] [command...]
#
# macosx builds the same libraries for the desktop, where the in-process
# shell runs under POOPTERM_IN_PROCESS=1 for testing.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
ports="$root/target/ios-ports"
sdk=${1:-iphonesimulator}
[ $# -gt 0 ] && shift
# POOPTERM_COMMANDS_OUT and POOPTERM_COMMANDS_LIST (name:directory entries)
# let ios/packages/build.sh build packages' commands the same way.
out=${POOPTERM_COMMANDS_OUT:-$root/target/ios-commands/$sdk}
logs="$root/target/ios-commands/logs-$sdk"
mkdir -p "$out" "$logs"

# iOS builds add headers its SDK lacks (compat-ios).
ios="-I $here/compat-ios -include $here/compat-ios/poopterm_ios.h"
case $sdk in
iphonesimulator) target=arm64-apple-ios17.0-simulator ;;
iphoneos) target=arm64-apple-ios17.0 ;;
macosx) target=arm64-apple-macos13.0 ios="" ;;
*) echo "unknown SDK $sdk" >&2; exit 2 ;;
esac

# name:directory of its sources (every .c file there is compiled).
all="
uname:shell_cmds/uname
echo:shell_cmds/echo
pwd:shell_cmds/pwd
env:shell_cmds/env
printenv:shell_cmds/printenv
date:shell_cmds/date
seq:shell_cmds/seq
basename:shell_cmds/basename
dirname:shell_cmds/dirname
yes:shell_cmds/yes
true:shell_cmds/true
false:shell_cmds/false
sleep:shell_cmds/sleep
printf:shell_cmds/printf
tee:shell_cmds/tee
expr:shell_cmds/expr
realpath:shell_cmds/realpath
which:shell_cmds/which
ls:file_cmds/ls
cp:file_cmds/cp
mv:file_cmds/mv
rm:file_cmds/rm
mkdir:file_cmds/mkdir
rmdir:file_cmds/rmdir
touch:file_cmds/touch
ln:file_cmds/ln
chmod:file_cmds/chmod
du:file_cmds/du
df:file_cmds/df
stat:file_cmds/stat
cat:text_cmds/cat
head:text_cmds/head
tail:text_cmds/tail
wc:text_cmds/wc
sort:text_cmds/sort
uniq:text_cmds/uniq
cut:text_cmds/cut
tr:text_cmds/tr
rev:text_cmds/rev
nl:text_cmds/nl
fold:text_cmds/fold
paste:text_cmds/paste
comm:text_cmds/comm
pr:text_cmds/pr
grep:text_cmds/grep
sed:text_cmds/sed
find:shell_cmds/find
xargs:shell_cmds/xargs
test:shell_cmds/test
mktemp:shell_cmds/mktemp
hexdump:shell_cmds/hexdump
getopt:shell_cmds/getopt
hostname:shell_cmds/hostname
logname:shell_cmds/logname
jot:shell_cmds/jot
what:shell_cmds/what
column:text_cmds/column
expand:text_cmds/expand
unexpand:text_cmds/unexpand
fmt:text_cmds/fmt
join:text_cmds/join
split:text_cmds/split
csplit:text_cmds/csplit
col:text_cmds/col
colrm:text_cmds/colrm
lam:text_cmds/lam
look:text_cmds/look
rs:text_cmds/rs
vis:text_cmds/vis
unvis:text_cmds/unvis
ed:text_cmds/ed
banner:text_cmds/banner
cksum:file_cmds/cksum
gzip:file_cmds/gzip
dd:file_cmds/dd
truncate:file_cmds/truncate
pathchk:file_cmds/pathchk
mkfifo:file_cmds/mkfifo
md5:text_cmds/md5
sha1:text_cmds/md5
sha224:text_cmds/md5
sha256:text_cmds/md5
sha384:text_cmds/md5
sha512:text_cmds/md5
base64:text_cmds/bintrans
bintrans:text_cmds/bintrans
uuencode:text_cmds/bintrans
uudecode:text_cmds/bintrans
b64encode:text_cmds/bintrans
b64decode:text_cmds/bintrans
ul:text_cmds/ul
time:shell_cmds/time
lockf:shell_cmds/lockf
whereis:shell_cmds/whereis
pax:file_cmds/pax
tar:file_cmds/pax
cpio:file_cmds/pax
compress:file_cmds/compress
uncompress:file_cmds/compress
install:file_cmds/install
chown:file_cmds/chown
chgrp:file_cmds/chown
xattr:file_cmds/xattr
awk:awk/src
less:less/less
stty:adv_cmds/stty
tty:adv_cmds/tty
tabs:adv_cmds/tabs
whois:adv_cmds/whois
cal:misc_cmds/ncal
ncal:misc_cmds/ncal
tsort:misc_cmds/tsort
diff:gnudiff/diffutils/src
cmp:gnudiff/diffutils/src
diff3:gnudiff/diffutils/src
sdiff:gnudiff/diffutils/src
"

all=${POOPTERM_COMMANDS_LIST:-$all}
wanted=${*:-}
failed=""
for entry in $all; do
	name=${entry%%:*}
	dir=$ports/${entry#*:}
	if [ -n "$wanted" ] && ! printf ' %s ' "$wanted" | grep -q " $name "; then
		continue
	fi
	# time reads its child's use through libproc, which iOS keeps private.
	if [ "$sdk" != macosx ] && [ "$name" = time ]; then
		continue
	fi
	if [ ! -d "$dir" ]; then
		echo "missing $dir: clone apple-oss-distributions sources into target/ios-ports" >&2
		exit 1
	fi
	# Test programs and other tools that share the directory are left out
	# (but `test` itself is not).
	sources=$(find "$dir" -maxdepth 1 -name '*.c' \( ! -name '*test*' -o -name "$name.c" \) | sort)
	# Grammars become C first.
	for grammar in $(find "$dir" -maxdepth 1 -name '*.y'); do
		generated="$logs/$(basename "$grammar" .y).c"
		yacc -o "$generated" "$grammar"
		sources="$sources $generated"
	done
	extra="" libraries=""
	case $name in
	ls) extra='-DCOLORLS' ;;
	sort) extra='-DSORT_VERSION="2.3-Apple" -DWITHOUT_NLS' ;;
	# grep reads compressed files through the system's libraries.
	grep) libraries='-lz -lbz2 -llzma' ;;
	# gzip includes its decoders' files itself; .xz and .lz are left out.
	gzip)
		sources="$dir/gzip.c"
		extra='-DNO_XZ_SUPPORT -DNO_LZ_SUPPORT -DGZIP_APPLE_VERSION="poopterm" -include stdbool.h'
		libraries='-lz -lbz2'
		;;
	join) extra='-include stdbool.h' ;;
	# pr has a global named err, so leaves err.h out.
	pr) extra='-DPOOPTERM_NO_ERR_H' ;;
	# The system's humanize_number has no IEC prefixes (KiB): plain K.
	# gen.c is a program that builds dd's tables, not part of dd.
	dd)
		sources=$(printf '%s\n' $sources | grep -v '/gen\.c$')
		extra='-DHN_IEC_PREFIXES=0'
		;;
	# The One True AWK: its grammar and table come generated; maketab is the
	# program that generates the table.
	awk)
		sources="$dir/b.c $dir/lex.c $dir/lib.c $dir/main.c $dir/parse.c $dir/run.c $dir/tran.c $dir/proctab.c $dir/awkgram.tab.c"
		extra='-DHAS_ISBLANK'
		libraries='-lm'
		;;
	# less's settings are Apple's defines.h, one directory up; lessecho
	# and lesskey are programs of their own.
	less)
		sources=$(printf '%s\n' $sources | grep -v -e '/lessecho\.c$' -e '/lesskey\.c$' -e '/regexp\.c$' -e '/scrsize\.c$')
		extra="-I $dir/.. -DSYSDIR=\"/usr/share\" -DPOOPTERM_DEV_TTY"
		;;
	# GNU diffutils with Apple's config.h: each program is one file of src
	# plus the shared ones and lib's portability files.
	diff | cmp | diff3 | sdiff)
		top="$dir/.."
		shared=""
		[ "$name" = diff ] && shared="analyze.c context.c dir.c ed.c ifdef.c io.c normal.c side.c util.c"
		sources="$dir/$name.c $dir/version.c"
		for file in $shared; do sources="$sources $dir/$file"; done
		for file in c-stack cmpbuf error exclude exitfail freesoft hard-locale imaxtostr offtostr posixver prepargs quotesys regex setmode umaxtostr xmalloc xstrtol xstrtoumax basename strftime; do
			sources="$sources $top/lib/$file.c"
		done
		# GNU's regex.h, made from its template, ahead of the system's.
		mkdir -p "$logs/gnudiff-include"
		cp "$top/lib/regex.hin" "$logs/gnudiff-include/regex.h"
		extra="-DHAVE_CONFIG_H -I $logs/gnudiff-include -I $top -I $top/lib -DDEFAULT_DIFF_PROGRAM=\"diff\" -DLOCALEDIR=\"/usr/share/locale\""
		;;
	esac
	if xcrun -sdk "$sdk" clang -target "$target" -dynamiclib -Os -w \
		-include "$here/poopterm_cmd.h" $ios -I "$here/compat" -I "$dir" \
		-D__FBSDID\(x\)= -D_DARWIN_C_SOURCE $extra \
		-Wl,-dead_strip_dylibs -lutil -lxo -lncurses $libraries \
		-install_name "@rpath/$name" \
		-o "$out/$name" "$here/poopterm_cmd.c" $sources 2>"$logs/$name.log"; then
		echo "built $name"
	else
		echo "FAILED $name (see $logs/$name.log)"
		failed="$failed $name"
	fi
done
[ -z "$failed" ] || { echo "failed:$failed" >&2; exit 1; }
