#!/bin/sh
# Builds file (https://www.darwinsys.com/file/, BSD-2-Clause) as a
# Poopterm command library: its sources as they come, with the
# hand-written config.h of this directory, and its magic database,
# compiled by a copy of file built for this Mac, in share/file, where the
# shell's MAGIC points.
#
#   ios/packages/file/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/file (the FILE5_46 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/file"
[ -d "$source" ] || { echo "missing $source: clone file FILE5_46 there" >&2; exit 1; }
work="$(dirname "$2")/../build"
rm -rf "$work"
mkdir -p "$work"
sed 's/X\.YY/546/' "$source/src/magic.h.in" >"$work/magic.h"
files=""
for name in apprentice ascmagic buffer cdf cdf_time compress der encoding file fsmagic \
	funcs is_csv is_json is_simh is_tar magic print readcdf readelf softmagic; do
	files="$files $source/src/$name.c"
done
flags="-I $here -I $work -I $source/src -DHAVE_CONFIG_H -DMAGIC=\"/usr/share/file/magic\""
# The database, by a file built to run here.
# shellcheck disable=SC2086
# A Mac target, whatever deployment target the caller exported.
env -u IPHONEOS_DEPLOYMENT_TARGET xcrun -sdk macosx clang -target arm64-apple-macos13.0 \
	-w -Os $flags $files -lz -o "$work/file-host"
(cd "$work" && ./file-host -C -m "$source/magic/Magdir")
share="$(dirname "$2")/../share"
mkdir -p "$share/file" "$share/doc/file"
mv "$work/Magdir.mgc" "$share/file/magic.mgc"
# shellcheck disable=SC2086
"$here/../cc.sh" "$1" "$2" $flags $files -lz
cp "$source/COPYING" "$share/doc/file/COPYING"
rm -rf "$work"
