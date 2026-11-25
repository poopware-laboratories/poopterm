#!/bin/sh
# Builds Info-ZIP's Zip 3.0 (Info-ZIP license) as a Poopterm command
# library, its sources as they come, with the flags of its macOS build.
#
#   ios/packages/zip/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/zip
#
# The source is expected in target/ios-ports/zip30.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/zip30"
[ -d "$source" ] || { echo "missing $source: unpack zip30 there" >&2; exit 1; }
bin=$(dirname "$2")
files=""
for name in zip zipfile zipup fileio util crc32 crypt ttyio globals deflate trees unix/unix; do
	files="$files $source/$name.c"
done
# BSD comes from <sys/param.h> as a version number, which unix/unix.c needs.
"$here/../cc.sh" "$1" "$2" -Wno-error=implicit-function-declaration -Wno-error=implicit-int -Wno-error=int-conversion -Wno-error=incompatible-function-pointer-types -DUNIX -DNO_ASM -DPOOPTERM_DEV_TTY -I "$source" $files
license="$bin/../share/doc/zip"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
