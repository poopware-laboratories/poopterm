#!/bin/sh
# Builds nyancat (https://github.com/klange/nyancat, NCSA) as a Poopterm
# command library, its source as it comes.
#
#   ios/packages/nyancat/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/nyancat (the 1.5.2 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/nyancat"
[ -d "$source" ] || { echo "missing $source: clone nyancat 1.5.2 there" >&2; exit 1; }
"$here/../cc.sh" "$1" "$2" "$source/src/nyancat.c"
license="$(dirname "$2")/../share/doc/nyancat"
mkdir -p "$license"
# Its license is the comment that opens the source.
sed -n '1,/\*\//p' "$source/src/nyancat.c" >"$license/LICENSE"
