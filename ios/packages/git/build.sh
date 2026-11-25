#!/bin/sh
# Builds git (https://git-scm.com, GPL-2.0) as a Poopterm command library:
# the git program with its built-in commands, from its release sources,
# with the settings git's Makefile picks for Darwin (config.mak.uname) and
# its generated headers made by prepare.py. Without libcurl, so it works on
# local repositories; a pager or editor is not started (GIT_PAGER=cat and
# -m are needed), as no second program runs beside it.
#
#   ios/packages/git/build.sh <iphonesimulator|iphoneos|macosx> <output>
#
# The source is expected in target/ios-ports/git-2.47.1 (kernel.org).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/git-2.47.1"
build="$root/target/ios-ports/build-git-$1"
[ -d "$source" ] || { echo "missing $source: unpack git-2.47.1.tar.gz there" >&2; exit 1; }
listed=$(python3 -I "$here/prepare.py" "$source" "$build")
sources=""
for file in $listed git.c common-main.c sha1dc_git.c sha1dc/sha1.c sha1dc/ubc_check.c \
	sha256/block/sha256.c compat/precompose_utf8.c compat/memmem.c compat/open.c compat/fopen.c \
	unix-socket.c unix-stream-server.c compat/simple-ipc/ipc-shared.c \
	compat/simple-ipc/ipc-unix-socket.c compat/qsort_s.c compat/regcomp_enhanced.c compat/stub/procinfo.c; do
	sources="$sources $source/$file"
done
version=2.47.1
"$here/../cc.sh" "$1" "$2" -I "$build" -I "$source" \
	-DPOOPTERM_NO_ERR_H -DNO_GETTEXT -DNO_OPENSSL -DNO_CURL -DNO_EXPAT -DSUPPORTS_SIMPLE_IPC -DNO_PERL -DNO_PYTHON \
	-DNO_MEMMEM -DUSE_ST_TIMESPEC -DHAVE_DEV_TTY -DHAVE_BSD_SYSCTL -DHAVE_GETDELIM \
	-DHAVE_NS_GET_EXECUTABLE_PATH -DHAVE_ARC4RANDOM -DHAVE_ALLOCA_H -DHAVE_PATHS_H \
	-DHAVE_CLOCK_GETTIME -DHAVE_CLOCK_MONOTONIC -DOPEN_RETURNS_EINTR -DFREAD_READS_DIRECTORIES \
	-DUSE_ENHANCED_BASIC_REGULAR_EXPRESSIONS -DPRECOMPOSE_UNICODE -DPROTECT_HFS_DEFAULT=1 \
	-DSHA1_DC -DSHA1DC_NO_STANDARD_INCLUDES -DSHA1DC_INIT_SAFE_HASH_DEFAULT=0 \
	-DSHA1DC_FORCE_ALIGNED_ACCESS \
	'-DSHA1DC_CUSTOM_INCLUDE_SHA1_C="git-compat-util.h"' \
	'-DSHA1DC_CUSTOM_INCLUDE_UBC_CHECK_C="git-compat-util.h"' -DSHA256_BLK \
	"-DGIT_VERSION=\"$version\"" "-DGIT_USER_AGENT=\"git/$version\"" '-DGIT_BUILT_FROM_COMMIT=""' \
	'-DGIT_HOST_CPU="arm64"' '-DGIT_EXEC_PATH="libexec/git-core"' '-DBINDIR="bin"' \
	'-DPREFIX="/"' '-DFALLBACK_RUNTIME_PREFIX="/"' '-DETC_GITCONFIG="etc/gitconfig"' \
	'-DETC_GITATTRIBUTES="etc/gitattributes"' '-DGIT_HTML_PATH="share/doc/git-doc"' \
	'-DGIT_MAN_PATH="share/man"' '-DGIT_INFO_PATH="share/info"' '-DGIT_LOCALE_PATH="share/locale"' \
	'-DDEFAULT_GIT_TEMPLATE_DIR="share/git-core/templates"' '-DSHELL_PATH="/bin/sh"' \
	'-DPAGER_ENV="LESS=FRX LV=-c"' '-DDEFAULT_PAGER="cat"' '-DDEFAULT_EDITOR="vi"' \
	$sources -lz -liconv -framework CoreFoundation
license="$(dirname "$2")/../share/doc/git"
mkdir -p "$license"
cp "$source/COPYING" "$license/LICENSE"
