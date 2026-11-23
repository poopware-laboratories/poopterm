#!/bin/sh
# Builds Poopterm packages: each a tar.gz of command libraries in bin/
# (and, for ports, their license in share/doc/<name>/),
# built from source like the bundled commands (ios/commands/build.sh), and
# an index listing each with its version, SHA-256, size and summary, for
# `pkg source` to point at.
#
#   ios/packages/build.sh [iphonesimulator|iphoneos|macosx] [package...]
#
# Output: target/ios-packages/<sdk>/{index,<name>-<version>.tar.gz}.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
sdk=${1:-iphonesimulator}
[ $# -gt 0 ] && shift
out="$root/target/ios-packages/$sdk"
work="$root/target/ios-packages/work-$sdk"
mkdir -p "$out"

# name|version|commands (name:directory, or "port": ios/packages/<name>/
# build.sh builds it; version "source" reads the version from its source)
packages="
jot|1.0|jot:shell_cmds/jot|Print sequential or random data
hexdump|1.0|hexdump:shell_cmds/hexdump|Show a file's bytes in hex, octal or ASCII
fastfetch|source|port|Show system information beside a logo
"

wanted=${*:-}
index="$out/index.new"
echo "# Poopterm packages for $sdk" >"$index"
# Building some packages keeps the others' entries.
if [ -n "$wanted" ] && [ -f "$out/index" ]; then
	grep -v '^#' "$out/index" | while read -r listed rest; do
		printf ' %s ' "$wanted" | grep -q " $listed " || echo "$listed $rest"
	done >>"$index"
fi
echo "$packages" | while IFS='|' read -r name version commands summary; do
	[ -n "$name" ] || continue
	if [ -n "$wanted" ] && ! printf ' %s ' "$wanted" | grep -q " $name "; then
		continue
	fi
	rm -rf "$work/$name"
	mkdir -p "$work/$name/bin"
	if [ "$commands" = port ]; then
		"$here/$name/build.sh" "$sdk" "$work/$name/bin/$name" >/dev/null
	else
		POOPTERM_COMMANDS_OUT="$work/$name/bin" POOPTERM_COMMANDS_LIST="$commands" \
			"$root/ios/commands/build.sh" "$sdk" >/dev/null
	fi
	if [ "$version" = source ]; then
		version=$(sed -n 's/^ *VERSION \([0-9.]*\).*/\1/p' "$root/target/ios-ports/$name/CMakeLists.txt" | head -1)
	fi
	archive="$name-$version.tar.gz"
	COPYFILE_DISABLE=1 tar -czf "$out/$archive" -C "$work/$name" .
	hash=$(shasum -a 256 "$out/$archive" | cut -d' ' -f1)
	size=$(wc -c <"$out/$archive" | tr -d ' ')
	echo "$name $version $hash $size $archive $summary" >>"$index"
	echo "packaged $name $version"
done
mv "$index" "$out/index"
echo "index: $out/index"
