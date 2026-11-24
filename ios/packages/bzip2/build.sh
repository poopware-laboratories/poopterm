#!/bin/sh
# Builds bzip2 1.0.8 (BSD-style license) as Poopterm command libraries:
# its sources as they come. bzip2 reads its own name to decide whether to
# compress or decompress, so bunzip2 and bzcat are copies of the same
# library, found under those names.
#
#   ios/packages/bzip2/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/bzip2
#
# The source is expected in target/ios-ports/bzip2-1.0.8.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/bzip2-1.0.8"
[ -d "$source" ] || { echo "missing $source: unpack bzip2-1.0.8 there" >&2; exit 1; }
bin=$(dirname "$2")
"$here/../cc.sh" "$1" "$2" -I "$source" -D_FILE_OFFSET_BITS=64 \
	"$source/blocksort.c" "$source/bzlib.c" "$source/compress.c" "$source/crctable.c" \
	"$source/decompress.c" "$source/huffman.c" "$source/randtable.c" "$source/bzip2.c"
cp "$2" "$bin/bunzip2"
cp "$2" "$bin/bzcat"
license="$bin/../share/doc/bzip2"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
