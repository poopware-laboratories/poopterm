#!/bin/sh
# Builds jq (https://jqlang.github.io/jq, MIT) as a Poopterm command library
# from its release sources, with our own configuration (jq_config.h) and the
# two small files its Makefile makes with sed made here the same way.
#
#   ios/packages/jq/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/jq-1.7.1 (its release tarball).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/jq-1.7.1"
generated="$root/target/ios-ports/build-jq-$1/src"
[ -d "$source" ] || { echo "missing $source: unpack jq-1.7.1.tar.gz there" >&2; exit 1; }
mkdir -p "$generated"
# builtin.jq as C string lines, as the Makefile's sed writes them.
sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/^/"/' -e 's/$/\\n"/' "$source/src/builtin.jq" >"$generated/builtin.inc"
echo '#define JQ_CONFIG "poopterm"' >"$generated/config_opts.inc"
sources=$(ls "$source"/src/*.c | grep -v '/inject_errors\.c$')
"$here/../cc.sh" "$1" "$2" -include "$here/jq_config.h" -I "$generated/.." -I "$source" -I "$source/src" \
	$sources "$source/src/decNumber/decContext.c" "$source/src/decNumber/decNumber.c"
license="$(dirname "$2")/../share/doc/jq"
mkdir -p "$license"
cp "$source/COPYING" "$license/LICENSE"
