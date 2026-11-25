#!/bin/sh
# Builds the ssh server of Dropbear (MIT) as `sshd`, a library the app
# calls itself (sshd_engine.c), not a command: its sessions run in the
# app's shell. Dropbear's sources are patched by patch_server.py.
#
#   ios/packages/dropbear/sshd.sh <iphonesimulator|iphoneos|macosx> <output>/bin/sshd
#
# The source is expected in target/ios-ports/dropbear-2026.94.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/dropbear-2026.94"
sdk=$1
bin=$(dirname "$2")
work="$root/target/ios-ports/build-sshd-$sdk"
rm -rf "$work"
mkdir -p "$work/src" "$bin"
python3 -I "$here/patch_server.py" "$source/src" "$work/src"
sed 's/^\( *#define \([^ ][^ ]*\) .*\)/#ifndef \2\
\1\
#endif/' "$source/src/default_options.h" >"$work/default_options_guard.h"

# This server's options come before the client's in $here.
flags="-I $here/server -I $work -DHAVE_CONFIG_H -DLOCALOPTIONS_H_EXISTS -DHAVE_CRYPT=1 -include $here/poop_exit.h \
-I $here -I $work/src -I $source/src -I $source/libtomcrypt/src/headers -I $source/libtommath"

ltc=$(sed -n '/^OBJECTS=/,/^$/p' "$source/libtomcrypt/Makefile.in" | sed 's/^OBJECTS=//' | tr ' \\' '\n\n' | grep '\.o$' | sed 's/\.o$/.c/' | grep -v 'aes_enc')
ltm=$(sed -n '/^OBJECTS=/,/^$/p' "$source/libtommath/Makefile.in" | sed 's/^OBJECTS=//' | tr ' \\' '\n\n' | grep '\.o$' | sed 's/\.o$/.c/')
ltc_files=""
for f in $ltc; do ltc_files="$ltc_files $source/libtomcrypt/$f"; done
ltm_files=""
for f in $ltm; do ltm_files="$ltm_files $source/libtommath/$f"; done
export NO_SHIM=1
"$here/../cc.sh" "$sdk" "$work/libtomcrypt.a" $flags -DLTC_SOURCE $ltc_files
"$here/../cc.sh" "$sdk" "$work/libtommath.a" $flags $ltm_files

common="dbutil buffer dbhelpers dss bignum signkey rsa dbrandom queue atomicio compat fake-rfc2553 \
ltc_prng ecc ecdsa sk-ecdsa crypto_desc curve25519 ed25519 sk-ed25519 dbmalloc dbctype \
gensignkey gendss genrsa gened25519"
shared="common-session packet common-algo common-kex common-channel common-chansession termcodes \
loginrec tcp-accept listener process-packet dh_groups common-runopts circbuffer list netio chachapoly \
gcm kex-x25519 kex-dh kex-ecdh kex-pqhybrid sntrup761 mlkem768"
server="svr-kex svr-auth svr-authpasswd svr-authpubkey svr-authpubkeyoptions svr-chansession svr-runopts \
svr-service svr-session svr-forward svr-streamfwd sshpty"
files="$here/sshd_engine.c"
for name in $server; do
	if [ -f "$work/src/$name.c" ]; then files="$files $work/src/$name.c"; else files="$files $source/src/$name.c"; fi
done
for name in $shared $common; do files="$files $source/src/$name.c"; done
"$here/../cc.sh" "$sdk" "$bin/sshd" $flags $files "$work/libtomcrypt.a" "$work/libtommath.a" -lz
echo "built $bin/sshd"
