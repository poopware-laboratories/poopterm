#!/bin/sh
# Builds TinyCC (https://repo.or.cz/tinycc.git, LGPL-2.1) as a Poopterm
# command library, `tcc`, which compiles C and runs it in memory
# (`tcc -run file.c`): its sources with the changes of tcc-ios.patch
# (applied with patch), the hand-written config.h of this directory, and
# a launcher (ptcc_main.c). The folder it needs at run time, with its
# headers and the sources of its runtime helpers, goes in share/tcc beside
# the command; the app points TCCDIR at it.
#
#   ios/packages/tcc/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/tcc
#
# The source is expected in target/ios-ports/tinycc (git clone, mob branch).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/tinycc"
[ -d "$source" ] || { echo "missing $source: git clone https://repo.or.cz/tinycc.git there" >&2; exit 1; }
sdk=$1
bin=$(dirname "$2")
share="$bin/../share/tcc"
work="$root/target/ios-ports/build-tcc-$sdk"
rm -rf "$work"
mkdir -p "$work" "$bin" "$share/include" "$share/lib"
(cd "$source" && tar --exclude=.git -cf - .) | (cd "$work" && mkdir src && cd src && tar -xf -)
(cd "$work/src" && patch -p1 -s <"$here/tcc-ios.patch")

# tcc.c is the whole compiler (ONE_SOURCE) with main renamed, built as a
# library of its own so the shim does not rename that main again.
"$here/../cc.sh" "$sdk" "$work/libtcc_main.a" \
	-DHAVE_CONFIG_H -DPOOPTERM_RUNTIME_SOURCES -DPOOPTERM_JIT -DPOOPTERM_KEEP_MAIN -Dmain=tcc_main \
	-I "$here" -I "$work/src" "$work/src/tcc.c"
"$here/../cc.sh" "$sdk" "$2" -I "$work/src" "$here/ptcc_main.c" "$work/libtcc_main.a"

cp "$source"/include/*.h "$share/include/"
cp -R "$here"/include/. "$share/include/"
for name in runmain libtcc1 lib-arm64 builtin; do
	cp "$source/lib/$name.c" "$share/lib/"
done
license="$bin/../share/doc/tcc"
mkdir -p "$license"
cp "$source/COPYING" "$license/COPYING"
echo "built $2"
