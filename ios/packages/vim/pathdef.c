/* Vim's auto/pathdef.c, written by hand: where its files are. The runtime
 * files are wherever $VIMRUNTIME says (the shell sets it, DATA_DIRS in
 * src/inproc); these are only the defaults when it is unset. */
#include "vim.h"

char_u *default_vim_dir = (char_u *)"/usr/share/vim";
char_u *default_vimruntime_dir = (char_u *)"";
char_u *all_cflags = (char_u *)"clang -Os (Poopterm's ios/packages/vim/build.sh)";
char_u *all_lflags = (char_u *)"clang -dynamiclib";
char_u *compiled_user = (char_u *)"poopterm";
char_u *compiled_sys = (char_u *)"poopterm";
