#!/bin/sh
# Builds FIGlet (https://github.com/cmatsuoka/figlet, BSD-3-Clause) as a
# Poopterm command library, its sources as they come, and puts its fonts
# in share/figlet, where the shell's FIGLET_FONTDIR points.
#
#   ios/packages/figlet/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/figlet (the 2.2.5 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/figlet"
[ -d "$source" ] || { echo "missing $source: clone figlet 2.2.5 there" >&2; exit 1; }
"$here/../cc.sh" "$1" "$2" -DDEFAULTFONTDIR='"fonts"' -DDEFAULTFONTFILE='"standard"' \
	"$source/figlet.c" "$source/zipio.c" "$source/crc.c" "$source/inflate.c" "$source/utf8.c"
share="$(dirname "$2")/../share"
mkdir -p "$share/figlet" "$share/doc/figlet"
cp "$source"/fonts/*.flf "$source"/fonts/*.flc "$share/figlet/"
cp "$source/LICENSE" "$share/doc/figlet/LICENSE"
