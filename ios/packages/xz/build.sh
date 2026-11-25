#!/bin/sh
# Builds XZ Utils (https://tukaani.org/xz/; xz is 0BSD, the rest of what
# is used too) as a Poopterm command library: the xz program and liblzma,
# their sources as they come, with the hand-written config.h of this
# directory, without threads.
#
#   ios/packages/xz/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/xz (the v5.8.1 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/xz/src"
[ -d "$source" ] || { echo "missing $source: clone xz v5.8.1 there" >&2; exit 1; }
lzma="$source/liblzma"
includes=""
for dir in api common check lz lzma rangecoder delta simple; do
	includes="$includes -I $lzma/$dir"
done
files=$(
	ls "$lzma"/common/*.c | grep -v -e '_mt\.c$' -e outqueue.c -e hardware_cputhreads.c
	echo "$lzma/check/check.c $lzma/check/crc32_fast.c $lzma/check/crc64_fast.c $lzma/check/sha256.c"
	ls "$lzma"/lz/*.c "$lzma"/delta/*.c "$lzma"/simple/*.c "$lzma"/rangecoder/price_table.c
	ls "$lzma"/lzma/*.c | grep -v tablegen
	ls "$source"/xz/*.c
	ls "$source"/common/tuklib_*.c
)
# shellcheck disable=SC2086
# -iquote: liblzma's own lzma.h, before the shim's stand-in (ios/commands/compat).
"$here/../cc.sh" "$1" "$2" -I "$here" -iquote "$lzma/api" -DHAVE_CONFIG_H -I "$source/common" \
	$includes $files
license="$(dirname "$2")/../share/doc/xz"
mkdir -p "$license"
cp "$source/../COPYING" "$source/../COPYING.0BSD" "$license/"
