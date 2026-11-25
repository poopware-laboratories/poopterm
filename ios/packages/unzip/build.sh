#!/bin/sh
# Builds Info-ZIP's UnZip 6.0 (Info-ZIP license) as Poopterm command
# libraries, its sources as they come, with the flags of its own macosx
# recipe. unzip reads its own name to act as zipinfo, so that is a copy.
#
#   ios/packages/unzip/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/unzip
#
# The source is expected in target/ios-ports/unzip60.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/unzip60"
[ -d "$source" ] || { echo "missing $source: unpack unzip60 there" >&2; exit 1; }
bin=$(dirname "$2")
files=""
for name in unzip crc32 crypt envargs explode extract fileio globals inflate list match process ttyio \
	ubz2err unreduce unshrink zipinfo unix/unix; do
	files="$files $source/$name.c"
done
# Old C (1990s): implicit declarations stay warnings. /dev/tty reads passwords.
# BSD comes from <sys/param.h> as a version number, which unix/unix.c needs.
"$here/../cc.sh" "$1" "$2" -Wno-error=implicit-function-declaration -Wno-error=implicit-int -Wno-error=int-conversion -Wno-error=incompatible-function-pointer-types -DBSD -DUNIX -DPOOPTERM_DEV_TTY -I "$source" $files
cp "$2" "$bin/zipinfo"
license="$bin/../share/doc/unzip"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
