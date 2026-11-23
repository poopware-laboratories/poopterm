#!/bin/sh
# Builds fastfetch (https://github.com/fastfetch-cli/fastfetch, MIT) as a
# Poopterm command library: its own CMake build, given an iOS branch by
# prepare.py, compiled with the command shim, then linked with it into one
# dynamic library.
#
#   ios/packages/fastfetch/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/fastfetch (git clone).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
sdk=$1
output=$2
source="$root/target/ios-ports/fastfetch"
build="$root/target/ios-ports/build-fastfetch-$sdk"
shim="$root/ios/commands"

if [ ! -d "$source" ]; then
	echo "missing $source: git clone https://github.com/fastfetch-cli/fastfetch there" >&2
	exit 1
fi
python3 "$here/prepare.py" "$source"

case $sdk in
iphonesimulator) system=iOS target=arm64-apple-ios17.0-simulator ;;
iphoneos) system=iOS target=arm64-apple-ios17.0 ;;
*) echo "unknown SDK $sdk (fastfetch's macOS build needs no port)" >&2; exit 2 ;;
esac

flags="SHELL:-include $shim/poopterm_cmd.h;-I$here/compat;-I$shim/compat-ios;SHELL:-include $shim/compat-ios/poopterm_ios.h"
cmake -S "$source" -B "$build" -G Ninja \
	-DCMAKE_SYSTEM_NAME="$system" -DCMAKE_OSX_SYSROOT="$sdk" \
	-DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=17.0 \
	-DCMAKE_BUILD_TYPE=Release -DBUILD_TESTS=OFF -DBUILD_FLASHFETCH=OFF \
	-DENABLE_IMAGE_LOGO=OFF -DENABLE_VULKAN=OFF -DENABLE_SQLITE3=OFF -DENABLE_WORDEXP=OFF \
	-DPOOPTERM_FASTFETCH_IOS="$here" -DPOOPTERM_FASTFETCH_FLAGS="$flags" >"$build.log"
ninja -C "$build" libfastfetch CMakeFiles/fastfetch.dir/src/fastfetch.c.o >>"$build.log"

objects=$(find "$build/CMakeFiles/libfastfetch.dir" "$build/CMakeFiles/fastfetch.dir" -name '*.o')
xcrun -sdk "$sdk" clang -target "$target" -dynamiclib -Os -w \
	-include "$shim/poopterm_cmd.h" $([ "$sdk" = macosx ] || echo "-I $shim/compat-ios -include $shim/compat-ios/poopterm_ios.h") \
	"$shim/poopterm_cmd.c" $objects \
	-framework CoreFoundation -framework Foundation -framework IOKit -framework Metal -framework UIKit \
	-Wl,-alias,_main,_poopterm_main -Wl,-dead_strip -install_name @rpath/fastfetch -o "$output"
# Its license travels with it, beside bin/.
license="$(dirname "$output")/../share/doc/fastfetch"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
echo "built $output"
