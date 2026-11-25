#!/bin/sh
# Builds SQLite's shell (https://sqlite.org, public domain) as a Poopterm
# command library: the amalgamation and shell.c as they come.
#
#   ios/packages/sqlite3/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/sqlite-autoconf-3470200.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/sqlite-autoconf-3470200"
[ -d "$source" ] || { echo "missing $source: unpack sqlite-autoconf-3470200.tar.gz there" >&2; exit 1; }
"$here/../cc.sh" "$1" "$2" -I "$source" -DSQLITE_THREADSAFE=0 -DSQLITE_OMIT_LOAD_EXTENSION \
	-DSQLITE_ENABLE_FTS5 -DSQLITE_ENABLE_MATH_FUNCTIONS "$source/sqlite3.c" "$source/shell.c"
license="$(dirname "$2")/../share/doc/sqlite3"
mkdir -p "$license"
echo "SQLite is in the public domain: https://sqlite.org/copyright.html" >"$license/LICENSE"
