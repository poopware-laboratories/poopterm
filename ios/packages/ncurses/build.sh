#!/bin/sh
# Builds ncurses 6.5 (MIT license) as a static library for Poopterm's
# command libraries: compiled with the command shim, so its calls to
# tcgetattr, tcsetattr, ioctl and isatty go to Poopterm's terminal device
# (the system libncurses calls the kernel itself, which an iPhone app's
# pipes cannot answer). Programs link this one instead.
#
#   ios/packages/ncurses/build.sh <iphonesimulator|iphoneos|macosx> <output directory>
#
# Output: <output>/lib/libncursesw.a and <output>/include/*.h. The source
# is expected in target/ios-ports/ncurses-6.5 (generate.sh makes the rest).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
src="$root/target/ios-ports/ncurses-6.5"
gen="$root/target/ios-ports/ncurses-gen"
sdk=$1
out=$2
[ -d "$src" ] || { echo "missing $src: unpack ncurses-6.5 there" >&2; exit 1; }
[ -f "$gen/include/curses.h" ] || "$here/generate.sh" "$src" "$gen" >/dev/null
mkdir -p "$out/lib" "$out/include"

# The library objects: the base, wide-character, extended and terminfo
# modules of the source's own list, and the files generated from it.
sources=$(awk '
	/^@ / { keep = ($2 == "base" || $2 == "widechar" || $2 == "ext_funcs" || $2 == "termlib" || $2 == "ticlib" || $2 == "ext_tinfo"); next }
	/^#/ || NF < 3 { next }
	keep { print $1 ":" $3 }
' "$src/ncurses/modules")
files=""
for entry in $sources; do
	name=${entry%%:*}
	# A test program of the source's own, built from the same list.
	[ "$name" = link_test ] && continue
	case ${entry#*:} in
	'$(base)') dir=$src/ncurses/base ;;
	'$(tinfo)') dir=$src/ncurses/tinfo ;;
	'$(wide)') dir=$src/ncurses/widechar ;;
	'$(serial)') dir=$src/ncurses/tty ;;
	'$(trace)') dir=$src/ncurses/trace ;;
	.) dir=$gen/ncurses ;;
	*) echo "unknown directory for $name" >&2; exit 1 ;;
	esac
	files="$files $dir/$name.c"
done
# What the sources include: the generated headers first, then the source's.
"$here/../cc.sh" "$1" "$out/lib/libncursesw.a" \
	-DHAVE_CONFIG_H -D_XOPEN_SOURCE_EXTENDED -DNCURSES_STATIC \
	-I "$here" -I "$gen/ncurses" -I "$gen/include" -I "$src/ncurses" -I "$src/include" \
	$files
cp "$gen/include/curses.h" "$gen/include/term.h" "$gen/include/termcap.h" \
	"$gen/include/unctrl.h" "$gen/include/ncurses_dll.h" "$out/include/"
mkdir -p "$out/share/doc/ncurses"
cp "$src/COPYING" "$out/share/doc/ncurses/COPYING"
echo "ncurses: $(ls "$out/lib")"
