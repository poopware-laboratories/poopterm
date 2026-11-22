/*
 * <termcap.h>, missing from the iOS SDK though its libncurses has the
 * functions: ls uses them for colors (`ls -G`). Without a terminal
 * database to read, `tgetent` fails and ls prints without colors.
 */
#ifndef POOPTERM_TERMCAP_H
#define POOPTERM_TERMCAP_H
int tgetent(char *buffer, const char *name);
char *tgetstr(const char *id, char **area);
char *tgoto(const char *cap, int col, int row);
int tputs(const char *str, int affcnt, int (*putc)(int));
#endif
