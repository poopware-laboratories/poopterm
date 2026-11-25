#!/bin/sh
# Builds lolcat (https://github.com/jaseg/lolcat, WTFPL) as a Poopterm
# command library, its source as it comes.
#
#   ios/packages/lolcat/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/lolcat (the v1.5 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/lolcat"
[ -d "$source" ] || { echo "missing $source: clone lolcat v1.5 there" >&2; exit 1; }
# Its main ends without a return, which C reads as 0.
"$here/../cc.sh" "$1" "$2" -std=c11 -DPOOPTERM_KEEP_MAIN -Wl,-alias,_main,_poopterm_main \
	"$source/lolcat.c"
license="$(dirname "$2")/../share/doc/lolcat"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
