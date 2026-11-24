#!/bin/sh
# Compiles a port's C sources with the command shim (ios/commands) and
# links them into one command library, as the bundled commands are built.
#
#   ios/packages/cc.sh <iphonesimulator|iphoneos|macosx> <output> [clang arguments...]
#
# The arguments are the port's sources, flags and libraries. An output
# ending in .a is a static library of the sources' objects instead (a
# port's library, compiled with flags of its own), for a later call to link.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
shim=$(cd "$here/../commands" && pwd)
sdk=$1
output=$2
shift 2
ios="-I $shim/compat-ios -include $shim/compat-ios/poopterm_ios.h"
case $sdk in
iphonesimulator) target=arm64-apple-ios17.0-simulator ;;
iphoneos) target=arm64-apple-ios17.0 ;;
macosx) target=arm64-apple-macos13.0 ios="" ;;
*) echo "unknown SDK $sdk" >&2; exit 2 ;;
esac
mkdir -p "$(dirname "$output")"
case $output in
*.a)
	objects="$output.objects"
	rm -rf "$objects" "$output"
	mkdir -p "$objects"
	flags="" sources=""
	for argument in "$@"; do
		case $argument in
		*.c) sources="$sources $argument" ;;
		*) flags="$flags $argument" ;;
		esac
	done
	for source in $sources; do
		# Sources of one name in two directories stay apart.
		object="$objects/$(echo "$source" | tr / _).o"
		# The port's own include directories come first: a library that is
		# itself ncurses must find its term.h before the shim's stand-in.
		xcrun -sdk "$sdk" clang -target "$target" -c -Os -w \
			-include "$shim/poopterm_cmd.h" $flags $ios -I "$shim/compat" -D_DARWIN_C_SOURCE \
			"$source" -o "$object"
	done
	xcrun -sdk "$sdk" ar rcs "$output" "$objects"/*.o
	exit 0
	;;
esac
xcrun -sdk "$sdk" clang -target "$target" -dynamiclib -Os -w \
	-include "$shim/poopterm_cmd.h" $ios -I "$shim/compat" -D_DARWIN_C_SOURCE \
	"$shim/poopterm_cmd.c" "$@" \
	-Wl,-dead_strip -install_name "@rpath/$(basename "$output")" -o "$output"
