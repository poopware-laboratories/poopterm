#!/bin/sh
# Builds Lua (https://www.lua.org, MIT) as a Poopterm command library: the
# interpreter, its sources as they come, with POSIX features and iOS's
# lack of system() (os.execute reports it unavailable).
#
#   ios/packages/lua/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/lua-5.4.7 (www.lua.org/ftp).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/lua-5.4.7"
[ -d "$source" ] || { echo "missing $source: unpack lua-5.4.7.tar.gz there" >&2; exit 1; }
sources=$(ls "$source"/src/*.c | grep -v '/luac\.c$')
"$here/../cc.sh" "$1" "$2" -DLUA_USE_POSIX -DLUA_USE_DLOPEN -DLUA_USE_IOS $sources
license="$(dirname "$2")/../share/doc/lua"
mkdir -p "$license"
sed -n '/^\* Copyright (C) 1994/,/SOFTWARE\./p' "$source/src/lua.h" | sed 's/^\* \{0,1\}//' >"$license/LICENSE"
