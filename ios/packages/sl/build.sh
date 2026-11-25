#!/bin/sh
# Builds sl (https://github.com/mtoyoda/sl, Toyoda Masashi's license) as a
# Poopterm command library, its source as it comes, over our own ncurses.
#
#   ios/packages/sl/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/sl (the 5.02 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/sl"
[ -d "$source" ] || { echo "missing $source: clone sl 5.02 there" >&2; exit 1; }
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
"$here/../cc.sh" "$1" "$2" -I "$curses/include" "$source/sl.c" "$curses/lib/libncursesw.a"
license="$(dirname "$2")/../share/doc/sl"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
cp "$curses/share/doc/ncurses/COPYING" "$license/COPYING.ncurses"
