/*
 * Included before every source file of a command built for Poopterm on
 * iOS (`clang -include poopterm_cmd.h`). A command runs inside the app, so
 * what would end or inspect a process is routed to the shim instead:
 * `exit` and the `err` family end the command, not the app, closing a
 * standard stream only flushes it, and the tab's terminal device answers
 * `isatty` and the window size.
 */
#ifndef POOPTERM_CMD_H
#define POOPTERM_CMD_H

/* The system's declarations first, so the macros below do not rename them. */
#include <err.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <unistd.h>

/* What the shell passes a command: its standard descriptors and terminal. */
struct poopterm_tty {
	int in, out, err;
	/* Whether the descriptors are the tab's terminal (not a file or pipe). */
	int is_tty;
	unsigned short cols, rows;
	/* The command's environment, `NAME=value` strings ending in NULL. */
	char **envp;
};

int poopterm_entry(int argc, char **argv, const struct poopterm_tty *tty);
void poopterm_exit(int status) __attribute__((noreturn));
int poopterm_isatty(int fd);
int poopterm_ioctl(int fd, unsigned long request, ...);
int poopterm_fclose(FILE *stream);

#define exit(status) poopterm_exit(status)
#define _exit(status) poopterm_exit(status)
#define err(status, ...) (warn(__VA_ARGS__), poopterm_exit(status))
#define errx(status, ...) (warnx(__VA_ARGS__), poopterm_exit(status))
#define errc(status, code, ...) (warnc(code, __VA_ARGS__), poopterm_exit(status))
#define verr(status, format, args) (vwarn(format, args), poopterm_exit(status))
#define verrx(status, format, args) (vwarnx(format, args), poopterm_exit(status))
#define isatty(fd) poopterm_isatty(fd)
#define ioctl(...) poopterm_ioctl(__VA_ARGS__)
/* Closing a standard stream (cat and others do at the end) only flushes
 * it: the streams are the app's, and the next command needs them. */
#define fclose(stream) poopterm_fclose(stream)
/* The command's own `main`, called by `poopterm_entry`. A `main` that
 * relies on C's implicit `return 0` keeps its name (POOPTERM_KEEP_MAIN)
 * and is linked as `poopterm_main` with `-Wl,-alias,_main,_poopterm_main`. */
#ifndef POOPTERM_KEEP_MAIN
#define main poopterm_main
#endif

#endif
