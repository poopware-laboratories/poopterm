/*
 * <term.h>, missing from the iOS SDK though libncurses has the terminfo
 * functions: what ncal uses (and termcap's, which it reaches through it).
 */
#ifndef POOPTERM_TERM_H
#define POOPTERM_TERM_H
#include "termcap.h"
int setupterm(const char *term, int fd, int *error);
char *tigetstr(const char *name);
int tigetnum(const char *name);
int tigetflag(const char *name);
char *tparm(const char *string, ...);
int putp(const char *string);
#endif
