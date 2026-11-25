#!/bin/sh
# Builds tree (https://oldmanprogrammer.net/source.php?dir=projects/tree,
# GPL-2.0-or-later) as a Poopterm command library, its sources as they come.
#
#   ios/packages/tree/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/tree (the 2.2.1 release).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/tree"
[ -d "$source" ] || { echo "missing $source: unpack tree 2.2.1 there" >&2; exit 1; }
"$here/../cc.sh" "$1" "$2" "$source"/*.c
license="$(dirname "$2")/../share/doc/tree"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
