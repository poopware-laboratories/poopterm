#!/bin/sh
# Builds zstd (https://github.com/facebook/zstd, BSD-3-Clause or GPL-2.0)
# as a Poopterm command library: the zstd program and its library, its
# sources as they come, single-threaded (each command runs alone).
#
#   ios/packages/zstd/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/zstd (the v1.5.7 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/zstd"
[ -d "$source" ] || { echo "missing $source: clone zstd v1.5.7 there" >&2; exit 1; }
lib="$source/lib"
# shellcheck disable=SC2046
"$here/../cc.sh" "$1" "$2" -I "$lib" -I "$lib/common" -DZSTD_DISABLE_ASM \
	-DZSTD_NOBENCH -DZSTD_NODICT -DZSTD_NOTRACE -DZSTD_LEGACY_SUPPORT=0 \
	-DBACKTRACE_ENABLE=0 \
	$(ls "$source"/programs/*.c | grep -v -e benchfn -e benchzstd -e datagen -e dibio) \
	"$lib"/common/*.c "$lib"/compress/*.c "$lib"/decompress/*.c
license="$(dirname "$2")/../share/doc/zstd"
mkdir -p "$license"
cp "$source/LICENSE" "$source/COPYING" "$license/"
