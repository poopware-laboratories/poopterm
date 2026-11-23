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
	if [ ! -d "$dir" ]; then
		echo "missing $dir: clone apple-oss-distributions sources into target/ios-ports" >&2
		exit 1
	fi
	# Test programs and other tools that share the directory are left out.
	sources=$(find "$dir" -maxdepth 1 -name '*.c' ! -name '*test*' | sort)
	# Grammars become C first.
	for grammar in $(find "$dir" -maxdepth 1 -name '*.y'); do
		generated="$logs/$(basename "$grammar" .y).c"
		yacc -o "$generated" "$grammar"
		sources="$sources $generated"
	done
	extra=""
	case $name in
	ls) extra='-DCOLORLS' ;;
	sort) extra='-DSORT_VERSION="2.3-Apple" -DWITHOUT_NLS' ;;
	esac
	if xcrun -sdk "$sdk" clang -target "$target" -dynamiclib -Os -w \
		-include "$here/poopterm_cmd.h" $ios -I "$here/compat" -I "$dir" \
		-D__FBSDID\(x\)= -D_DARWIN_C_SOURCE $extra \
		-Wl,-dead_strip_dylibs -lutil -lxo -lncurses \
		-install_name "@rpath/$name" \
		-o "$out/$name" "$here/poopterm_cmd.c" $sources 2>"$logs/$name.log"; then
		echo "built $name"
	else
		echo "FAILED $name (see $logs/$name.log)"
		failed="$failed $name"
	fi
done
[ -z "$failed" ] || { echo "failed:$failed" >&2; exit 1; }
