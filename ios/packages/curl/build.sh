#!/bin/sh
# Builds curl (https://curl.se, curl license, MIT-like) as a Poopterm
# command library: libcurl and the curl tool compiled together from their
# sources with our own configuration (curl_config.h): HTTPS through Secure
# Transport and the system's roots, and the system's zlib.
#
#   ios/packages/curl/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/curl-8.11.1 (curl.se/download),
# the last release with Secure Transport.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/curl-8.11.1"
[ -d "$source" ] || { echo "missing $source: unpack curl-8.11.1.tar.gz there" >&2; exit 1; }
library="$root/target/ios-ports/build-curl-$1/libcurl.a"
"$here/../cc.sh" "$1" "$library" -DHAVE_CONFIG_H -DCURL_STATICLIB -DBUILDING_LIBCURL \
	-I "$here" -I "$source/include" -I "$source/lib" \
	"$source"/lib/*.c "$source"/lib/vauth/*.c "$source"/lib/vtls/*.c "$source"/lib/vquic/*.c
# The tool compiles libcurl's helpers it uses (src/Makefile.inc's CURLX_CFILES).
curlx=""
for file in base64 curl_multibyte dynbuf nonblock strtoofft timediff version_win32 warnless; do
	curlx="$curlx $source/lib/$file.c"
done
# Only macOS has the proxy settings curl reads there.
[ "$1" = macosx ] && system="-framework SystemConfiguration" || system=""
"$here/../cc.sh" "$1" "$2" -DHAVE_CONFIG_H -DCURL_STATICLIB \
	-I "$here" -I "$source/include" -I "$source/lib" -I "$source/src" \
	"$source"/src/*.c $curlx "$library" -lz -framework Security -framework CoreFoundation $system
license="$(dirname "$2")/../share/doc/curl"
mkdir -p "$license"
cp "$source/COPYING" "$license/LICENSE"
