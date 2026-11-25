#!/bin/sh
# Builds ncdu (https://dev.yorhel.nl/ncdu, MIT) as a Poopterm command
# library: its C sources (the 1.x series) as they come, with the
# hand-written config.h of this directory, over our own ncurses.
#
#   ios/packages/ncdu/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/ncdu-1.22.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/ncdu-1.22"
[ -d "$source" ] || { echo "missing $source: unpack ncdu-1.22 there" >&2; exit 1; }
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
mkdir -p "$curses/compat"
echo '#include <curses.h>' >"$curses/compat/ncurses.h"
"$here/../cc.sh" "$1" "$2" -I "$here" -I "$curses/include" -I "$curses/compat" \
	-I "$source/deps" -DHAVE_CONFIG_H -D_XOPEN_SOURCE_EXTENDED -DNCURSES_WIDECHAR=1 \
	"$source"/src/*.c "$source/deps/strnatcmp.c" "$curses/lib/libncursesw.a"
license="$(dirname "$2")/../share/doc/ncdu"
mkdir -p "$license"
cp "$source/COPYING" "$license/COPYING"
cp "$curses/share/doc/ncurses/COPYING" "$license/COPYING.ncurses"
