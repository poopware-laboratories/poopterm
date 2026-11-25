#!/bin/sh
# Builds Dropbear (https://matt.ucc.asn.au/dropbear/, MIT) as Poopterm
# command libraries: `ssh`, its client, and `dropbearkey`, which makes
# keys. Its bundled libtomcrypt and libtommath become one static library,
# with hand-written config.h and localoptions.h in this directory.
#
#   ios/packages/dropbear/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/ssh
#
# dropbearkey and sshd (the server) are built beside the ssh at the path given.
#
# The source is expected in target/ios-ports/dropbear-2026.94.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/dropbear-2026.94"
sdk=$1
bin=$(dirname "$2")
out=$(dirname "$bin")
[ -d "$source" ] || { echo "missing $source: unpack dropbear-2026.94 there" >&2; exit 1; }
work="$root/target/ios-ports/build-dropbear-$sdk"
mkdir -p "$work" "$bin"

# options.h includes default_options_guard.h: default_options.h with each
# "#define X Y" wrapped in "#ifndef X ... #endif", so localoptions.h wins.
# (Dropbear's own script does this with sed; the same sed is run here.)
sed 's/^\( *#define \([^ ][^ ]*\) .*\)/#ifndef \2\
\1\
#endif/' "$source/src/default_options.h" >"$work/default_options_guard.h"

flags="-I $work -DHAVE_CONFIG_H -DLOCALOPTIONS_H_EXISTS -DPOOPTERM_DEV_TTY -I $here -I $source/src \
-I $source/libtomcrypt/src/headers -I $source/libtommath"

# The libraries' objects, from their own makefiles' lists.
ltc=$(sed -n '/^OBJECTS=/,/^$/p' "$source/libtomcrypt/Makefile.in" | sed 's/^OBJECTS=//' | tr ' \\' '\n\n' | grep '\.o$' | sed 's/\.o$/.c/' | grep -v 'aes_enc')
ltm=$(sed -n '/^OBJECTS=/,/^$/p' "$source/libtommath/Makefile.in" | sed 's/^OBJECTS=//' | tr ' \\' '\n\n' | grep '\.o$' | sed 's/\.o$/.c/')
ltc_files=""
for f in $ltc; do ltc_files="$ltc_files $source/libtomcrypt/$f"; done
ltm_files=""
for f in $ltm; do ltm_files="$ltm_files $source/libtommath/$f"; done
"$here/../cc.sh" "$sdk" "$work/libtomcrypt.a" $flags -DLTC_SOURCE $ltc_files
"$here/../cc.sh" "$sdk" "$work/libtommath.a" $flags $ltm_files

common="dbutil buffer dbhelpers dss bignum signkey rsa dbrandom queue atomicio compat fake-rfc2553 \
ltc_prng ecc ecdsa sk-ecdsa crypto_desc curve25519 ed25519 sk-ed25519 dbmalloc dbctype \
gensignkey gendss genrsa gened25519"
client="cli-main cli-auth cli-authpasswd cli-kex cli-session cli-runopts cli-chansession \
cli-authpubkey cli-tcpfwd cli-channel cli-authinteract cli-agentfwd cli-readconf"
shared="common-session packet common-algo common-kex common-channel common-chansession termcodes \
loginrec tcp-accept listener process-packet dh_groups common-runopts circbuffer list netio chachapoly \
gcm kex-x25519 kex-dh kex-ecdh kex-pqhybrid sntrup761 mlkem768"
sources() { for name in "$@"; do printf '%s ' "$source/src/$name.c"; done; }

# ssh keeps main's name for C's implicit return 0? No: it exits itself.
"$here/../cc.sh" "$sdk" "$bin/ssh" $flags $(sources $client $shared $common) "$work/libtomcrypt.a" "$work/libtommath.a" -lz
"$here/../cc.sh" "$sdk" "$bin/dropbearkey" $flags $(sources dropbearkey $shared $common) "$work/libtomcrypt.a" "$work/libtommath.a" -lz
# The ssh server, a library the app calls (sshd.sh).
"$here/sshd.sh" "$sdk" "$bin/sshd"
license="$out/share/doc/dropbear"
mkdir -p "$license"
cp "$source/LICENSE" "$license/LICENSE"
echo "built $bin"
