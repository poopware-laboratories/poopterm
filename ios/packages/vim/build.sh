#!/bin/sh
# Builds Vim (https://www.vim.org, the Vim license) as a Poopterm command
# library: its sources as they come, with the hand-written config.h,
# osdef.h and pathdef.c of this directory (what its configure script
# would generate), over our own ncurses, and the runtime files its syntax
# highlighting, file types and indenting need in share/vim, where the
# shell's VIMRUNTIME points.
#
#   ios/packages/vim/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/vim (the v9.1.1000 tag).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
vim="$root/target/ios-ports/vim"
[ -d "$vim/src" ] || { echo "missing $vim/src: clone vim v9.1.1000 there" >&2; exit 1; }
curses="$root/target/ios-ports/ncurses-out-$1"
"$here/../ncurses/build.sh" "$1" "$curses" >/dev/null
# A copy of its sources, with auto/ filled in (xdiff finds it as ../auto).
work="$(dirname "$2")/../build"
source="$work/src"
rm -rf "$work"
mkdir -p "$work"
cp -R "$vim/src" "$source"
cp "$here/config.h" "$here/osdef.h" "$source/auto/"
files=$(sed -n '/^BASIC_SRC = /,/^$/p' "$source/Makefile" | tr -s ' \t\\' '\n' |
	grep '\.c$' | grep -v pathdef | sed "s|^|$source/|")
# shellcheck disable=SC2086
"$here/../cc.sh" "$1" "$2" -DHAVE_CONFIG_H -I "$source" -I "$source/proto" \
	-I "$curses/include" -DPOOPTERM_NO_ERR_H \
	$files "$source"/xdiff/*.c "$here/pathdef.c" "$curses/lib/libncursesw.a" -liconv
share="$(dirname "$2")/../share"
mkdir -p "$share/vim" "$share/doc/vim"
for file in defaults.vim filetype.vim ftoff.vim ftplugin.vim ftplugof.vim indent.vim \
	indoff.vim scripts.vim; do
	cp "$vim/runtime/$file" "$share/vim/"
done
for dir in autoload colors ftplugin import indent plugin syntax; do
	cp -R "$vim/runtime/$dir" "$share/vim/$dir"
done
cp "$vim/LICENSE" "$share/doc/vim/LICENSE"
cp "$curses/share/doc/ncurses/COPYING" "$share/doc/vim/COPYING.ncurses"
rm -rf "$work"
