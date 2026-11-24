#!/bin/sh
# Builds nano (https://nano-editor.org, GPL-3.0-or-later) as a Poopterm
# command library: its sources as they come, with the hand-written
# config.h of this directory, over our own ncurses.
#
#   ios/packages/nano/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/nano-8.7.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/nano-8.7"
[ -d "$source" ] || { echo "missing $source: unpack nano-8.7 there" >&2; exit 1; }
# Our own ncurses (ios/packages/ncurses): the system's calls the kernel for
# the terminal's settings, which a command in the app cannot answer.
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
"$here/../cc.sh" "$1" "$2" -I "$here" -I "$curses/include" -I "$source/src" \
	-DHAVE_CONFIG_H -D_XOPEN_SOURCE_EXTENDED -DNCURSES_WIDECHAR=1 \
	"$source"/src/*.c "$curses/lib/libncursesw.a"
license="$(dirname "$2")/../share/doc/nano"
mkdir -p "$license"
cp "$source/COPYING" "$license/COPYING"
cp "$curses/share/doc/ncurses/COPYING" "$license/COPYING.ncurses"
