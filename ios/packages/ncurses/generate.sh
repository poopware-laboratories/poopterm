#!/bin/sh
# Makes the sources ncurses 6.5 generates when it is built, without its
# configure script (a downloaded one is never run): the header templates
# are filled in with the values below, and the awk and shell scripts of
# its makefiles run on the capability lists, as `make` would run them.
#
#   ios/packages/ncurses/generate.sh <ncurses-6.5 directory> <output directory>
#
# Output: <output>/include and <output>/ncurses, ready to compile against
# with ios/packages/ncurses/ncurses_cfg.h.
set -eu

src=$(cd "$1" && pwd)
mkdir -p "$2"
out=$(cd "$2" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$out/include" "$out/ncurses"
mkdir -p "$out/include" "$out/ncurses"
awk=awk
sh=/bin/sh
version=$(sed -n 's/^[0-9]*:[0-9]*:[0-9]*[[:space:]]*\([0-9.]*\)[[:space:]]*\([0-9]*\)/\1 \2/p' "$src/VERSION" | head -1)
major=${version%%.*}; rest=${version#*.}; minor=${rest%% *}; patch=${version##* }

# What configure would have worked out: a wide-character build with the
# extended functions and colors, on Darwin.
subst() {
	sed \
	-e "s|@NCURSES_MAJOR@|$major|g" -e "s|@NCURSES_MINOR@|$minor|g" -e "s|@NCURSES_PATCH@|$patch|g" \
	-e 's|@NCURSES_MOUSE_VERSION@|2|g' -e 's|@HAVE_STDINT_H@|1|g' -e 's|@HAVE_STDNORETURN_H@|1|g' \
	-e 's|@cf_cv_header_stdbool_h@|1|g' -e 's|@NCURSES_CONST@|const|g' -e 's|@NCURSES_INLINE@|inline|g' \
	-e 's|@NCURSES_OPAQUE@|0|g' -e 's|@NCURSES_OPAQUE_FORM@|0|g' -e 's|@NCURSES_OPAQUE_MENU@|0|g' \
	-e 's|@NCURSES_OPAQUE_PANEL@|0|g' -e 's|@NCURSES_WATTR_MACROS@|0|g' -e 's|@cf_cv_enable_reentrant@|0|g' \
	-e 's|@NCURSES_EXT_FUNCS@|1|g' -e 's|@cf_cv_enable_sigwinch@|1|g' -e 's|@BROKEN_LINKER@|0|g' \
	-e 's|@NCURSES_INTEROP_FUNCS@|0|g' -e 's|@NCURSES_SIZE_T@|short|g' -e 's|@NCURSES_TPARM_VARARGS@|1|g' \
	-e 's|@NCURSES_TPARM_ARG@|intptr_t|g' -e 's|@NCURSES_WCWIDTH_GRAPHICS@|0|g' -e 's|@NCURSES_CH_T@|cchar_t|g' \
	-e 's|@cf_cv_enable_lp64@|0|g' -e 's|@cf_cv_typeof_chtype@|uint32_t|g' -e 's|@cf_cv_typeof_mmask_t@|uint32_t|g' \
	-e 's|@cf_cv_type_of_bool@|unsigned char|g' -e 's|@USE_CXX_BOOL@|0|g' -e 's|@NCURSES_LIBUTF8@|0|g' \
	-e 's|@NEED_WCHAR_H@|1|g' -e 's|@NCURSES_WCHAR_T@|0|g' -e 's|@NCURSES_WINT_T@|0|g' \
	-e 's|@NCURSES_OK_WCHAR_T@||g' -e 's|@NCURSES_CCHARW_MAX@|5|g' -e 's|@NCURSES_EXT_COLORS@|1|g' \
	-e 's|@NCURSES_XNAMES@|1|g' -e 's|@GENERATED_EXT_FUNCS@|generated|g' -e 's|@NCURSES_SP_FUNCS@|0|g' \
	-e 's|@cf_cv_1UL@|1U|g' -e 's|@NCURSES_WRAP_PREFIX@|_nc_|g' -e 's|@NCURSES_OSPEED@|short|g' \
	-e 's|@NCURSES_SBOOL@|char|g' -e 's|@NCURSES_USE_DATABASE@|1|g' -e 's|@NCURSES_USE_TERMCAP@|0|g' \
	-e 's|@HAVE_TERMIOS_H@|1|g' -e 's|@HAVE_TCGETATTR@|1|g' -e 's|@HAVE_TERMIO_H@|0|g' \
	-e 's|@EXP_WIN32_DRIVER@|0|g' -e 's|@HAVE_VSSCANF@|1|g'
}

inc="$out/include"
subst <"$src/include/curses.h.in" >"$inc/curses.head"
subst <"$src/include/MKterm.h.awk.in" >"$inc/MKterm.h.awk"
subst <"$src/include/unctrl.h.in" >"$inc/unctrl.h"
subst <"$src/include/termcap.h.in" >"$inc/termcap.h"
subst <"$src/include/ncurses_dll.h.in" >"$inc/ncurses_dll.h"
caps="$src/include/Caps $src/include/Caps-ncurses"

{
	cat "$inc/curses.head"
	AWK=$awk $sh "$src/include/MKkey_defs.sh" $caps
	cat "$src/include/curses.wide"
	cat "$src/include/curses.tail"
} >"$inc/curses.h"
$awk -f "$inc/MKterm.h.awk" $caps >"$inc/term.h"
$sh "$src/include/edit_cfg.sh" "$here/ncurses_cfg.h" "$inc/term.h"
$sh "$src/include/MKhashsize.sh" $caps >"$inc/hashsize.h"
AWK=$awk $sh "$src/include/MKncurses_def.sh" "$src/include/ncurses_defs" >"$inc/ncurses_def.h"
AWK=$awk $sh "$src/include/MKparametrized.sh" $caps >"$inc/parametrized.h"
: >"$inc/config.h"

# Programs run here, while generating: the build machine's compiler.
n="$out/ncurses"
tinfo="$src/ncurses/tinfo"
base="$src/ncurses/base"
build="cc -DHAVE_CONFIG_H -DUSE_BUILD_CC -I$n -I$src/ncurses -I$inc -I$src/include -I$here -w"
cd "$n"
$awk -f "$tinfo/MKnames.awk" bigstrings=0 $caps >names.c
$awk -f "$tinfo/MKcodes.awk" bigstrings=0 $caps >codes.c
echo | $awk -f "$base/MKunctrl.awk" bigstrings=0 >unctrl.c
AWK=$awk USE_SIGWINCH=1 $sh "$tinfo/MKkeys_list.sh" $caps | LC_ALL=C sort >keys.list
$build -o make_keys "$tinfo/make_keys.c"
./make_keys keys.list >init_keytry.h
$awk -f "$base/MKkeyname.awk" use_sigwinch=1 bigstrings=0 keys.list >lib_keyname.c
$build -o make_hash "$tinfo/make_hash.c"
$sh -e "$tinfo/MKcaptab.sh" $awk 0 "$tinfo/MKcaptab.awk" $caps >comp_captab.c
$sh -e "$tinfo/MKuserdefs.sh" $awk 0 $caps >comp_userdefs.c
FGREP="grep -F" $sh -e "$base/MKlib_gen.sh" "cc -E -DHAVE_CONFIG_H -I$n -I$src/ncurses -I$inc -I$here" $awk generated <"$inc/curses.h" >lib_gen.c
$sh -e "$src/ncurses/tty/MKexpanded.sh" "cc -E" -DHAVE_CONFIG_H -I"$n" -I"$src/ncurses" -I"$inc" -I"$here" >expanded.c
$sh -e "$tinfo/MKfallback.sh" "" "" "" "" >fallback.c
rm -f make_keys make_hash
echo "ncurses: generated $(ls "$n" | wc -l | tr -d ' ') sources and $(ls "$inc" | wc -l | tr -d ' ') headers in $out"
