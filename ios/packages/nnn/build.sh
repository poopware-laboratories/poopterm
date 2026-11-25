#!/bin/sh
# Builds nnn (https://github.com/jarun/nnn, BSD-2-Clause), a file manager,
# as a Poopterm command library: its source as it comes, over our own
# ncurses, without readline, X11 or the FIFO its previews use.
#
#   ios/packages/nnn/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/nnn (the v5.1 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/nnn"
[ -d "$source" ] || { echo "missing $source: clone nnn v5.1 there" >&2; exit 1; }
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
mkdir -p "$curses/compat"
echo '#include <curses.h>' >"$curses/compat/ncurses.h"
"$here/../cc.sh" "$1" "$2" -I "$curses/include" -I "$curses/compat" \
	-D_XOPEN_SOURCE_EXTENDED -DNCURSES_WIDECHAR=1 -DNORL -DNOX11 -DNOFIFO -DNOSSN \
	-std=c11 -D_DARWIN_C_SOURCE "$source/src/nnn.c" "$curses/lib/libncursesw.a" -lpthread
license="$(dirname "$2")/../share/doc/nnn"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
cp "$curses/share/doc/ncurses/COPYING" "$license/COPYING.ncurses"
