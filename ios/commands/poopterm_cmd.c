/*
 * Linked into every command library: the entry Poopterm calls. It points
 * the standard descriptors at the tab's device for the command's run,
 * resets the state commands assume a fresh process has (getopt, stdio),
 * and turns `exit` into a return.
 */
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

#include "poopterm_cmd.h"

/* The command's `main`, renamed by the header. */
int poopterm_main(int argc, char **argv);

static jmp_buf finished;
static int status;
static struct poopterm_tty current;
extern char **environ;

void poopterm_exit(int code) {
	status = code;
	longjmp(finished, 1);
}

#undef isatty
int poopterm_isatty(int fd) {
	if (fd >= 0 && fd <= 2) {
		return current.is_tty;
	}
	return isatty(fd);
}

#undef ioctl
int poopterm_ioctl(int fd, unsigned long request, ...) {
	va_list args;
	va_start(args, request);
	void *arg = va_arg(args, void *);
	va_end(args);
	if (fd >= 0 && fd <= 2 && request == TIOCGWINSZ) {
		if (!current.is_tty) {
			errno = ENOTTY;
			return -1;
		}
		struct winsize *size = arg;
		size->ws_col = current.cols;
		size->ws_row = current.rows;
		size->ws_xpixel = 0;
		size->ws_ypixel = 0;
		return 0;
	}
	return ioctl(fd, request, arg);
}

#undef fclose
int poopterm_fclose(FILE *stream) {
	if (stream == stdin || stream == stdout || stream == stderr) {
		int failed = fflush(stream) != 0 || ferror(stream);
		clearerr(stream);
		return failed ? EOF : 0;
	}
	return fclose(stream);
}

#undef exit
int poopterm_entry(int argc, char **argv, const struct poopterm_tty *tty) {
	int saved[3];
	const int to[3] = {tty->in, tty->out, tty->err};
	fflush(NULL);
	for (int fd = 0; fd < 3; fd++) {
		saved[fd] = fcntl(fd, F_DUPFD_CLOEXEC, 3);
		dup2(to[fd], fd);
	}
	current = *tty;
	char **saved_environ = environ;
	environ = tty->envp;
	/* Messages from err() and warn() name the command, not the app. */
	const char *saved_name = getprogname();
	if (argc > 0) {
		setprogname(argv[0]);
	}
	fpurge(stdin);
	clearerr(stdin);
	clearerr(stdout);
	clearerr(stderr);
	optind = 1;
	optreset = 1;
	opterr = 1;
	status = 0;
	if (setjmp(finished) == 0) {
		status = poopterm_main(argc, argv);
	}
	fflush(stdout);
	fflush(stderr);
	environ = saved_environ;
	setprogname(saved_name);
	for (int fd = 0; fd < 3; fd++) {
		if (saved[fd] >= 0) {
			dup2(saved[fd], fd);
			close(saved[fd]);
		} else {
			close(fd);
		}
	}
	return status;
}
