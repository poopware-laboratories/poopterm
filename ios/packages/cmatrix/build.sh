#!/bin/sh
# Builds cmatrix (https://github.com/abishekvashok/cmatrix, GPL-3.0) as a
# Poopterm command library: its source as it comes, with the hand-written
# config.h of this directory, over our own ncurses.
#
#   ios/packages/cmatrix/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/cmatrix (the v2.0 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/cmatrix"
[ -d "$source" ] || { echo "missing $source: clone cmatrix v2.0 there" >&2; exit 1; }
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
# Its curses.h is ncurses.h here.
mkdir -p "$curses/compat"
echo '#include <curses.h>' >"$curses/compat/ncurses.h"
"$here/../cc.sh" "$1" "$2" -I "$here" -I "$curses/include" -I "$curses/compat" \
	"$source/cmatrix.c" "$curses/lib/libncursesw.a"
license="$(dirname "$2")/../share/doc/cmatrix"
mkdir -p "$license"
cp "$source/COPYING" "$license/COPYING"
cp "$curses/share/doc/ncurses/COPYING" "$license/COPYING.ncurses"
