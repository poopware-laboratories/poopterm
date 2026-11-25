#!/bin/sh
# Builds cpufetch (https://github.com/Dr-Noob/cpufetch, GPL-2.0) as a
# Poopterm command library: its ARM sources, which read an Apple chip
# through sysctl, taught by patch.py and apple_chip.m the chips its tables
# do not know (every iPhone and iPad, and Macs after the M3).
#
#   ios/packages/cpufetch/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/cpufetch (the v1.06 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
original="$root/target/ios-ports/cpufetch/src"
[ -d "$original" ] || { echo "missing $original: clone cpufetch v1.06 there" >&2; exit 1; }
source="$(dirname "$2")/../src"
rm -rf "$source"
cp -R "$original" "$source"
python3 -I "$here/patch.py" "$source"
common="main cpu udev printer args global soc pci sysctl"
arm="midr uarch soc udev sve"
files=$(for f in $common; do echo "$source/common/$f.c"; done; for f in $arm; do echo "$source/arm/$f.c"; done)
# shellcheck disable=SC2086
"$here/../cc.sh" "$1" "$2" -DARCH_ARM -std=c99 -Wno-unused-parameter \
	-DGIT_FULL_VERSION='"v1.06"' -fobjc-arc $files "$here/apple_chip.m" \
	-framework Metal -framework Foundation
license="$(dirname "$2")/../share/doc/cpufetch"
mkdir -p "$license"
cp "$original/../LICENSE" "$license/LICENSE"
rm -rf "$source"
