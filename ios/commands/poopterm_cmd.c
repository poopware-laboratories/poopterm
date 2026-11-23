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
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

#include "poopterm_cmd.h"

/* The command's `main`, renamed by the header. */
int poopterm_main(int argc, char **argv);

static jmp_buf finished;
static int status;
static struct poopterm_tty current;
/* Whether the command is running, so it cannot be started inside itself
 * (its globals and `finished` are this library's, one copy). */
static int running;
extern char **environ;

static void back_to_parent(int code) __attribute__((noreturn));

/* A child `fork` started, until its exec or `_exit` comes back. */
static struct {
	jmp_buf back;
	int active;
	pid_t pid;
	int saved[3];
	int cwd;
} forking;

void poopterm_exit(int code) {
	if (forking.active) {
		back_to_parent(code);
	}
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

/* Children that have ended, with their statuses, until waited for. */
#define POOPTERM_CHILDREN 64
static struct {
	pid_t pid;
	int code;
} ended[POOPTERM_CHILDREN];
static int ended_count;
/* Made-up process numbers, far from real ones. */
static pid_t next_pid = 1000000;

static void child_ended(pid_t pid, int code) {
	if (ended_count == POOPTERM_CHILDREN) {
		/* Nobody waited for the oldest: forget it. */
		memmove(&ended[0], &ended[1], sizeof ended[0] * (POOPTERM_CHILDREN - 1));
		ended_count--;
	}
	ended[ended_count].pid = pid;
	ended[ended_count].code = code & 0xff;
	ended_count++;
}

/* Runs `file` on `in`, `out` and `err` with `envp` (this command's own
 * environment when NULL): its status, or minus an errno. */
static int run_child(const char *file, char *const argv[], char *const envp[], int in, int out, int err) {
	if (current.spawn == NULL) {
		return -ENOSYS;
	}
	struct poopterm_tty tty = current;
	tty.in = in;
	tty.out = out;
	tty.err = err;
	tty.envp = (char **)(envp ? envp : environ);
	fflush(NULL);
	/* `stdin` is one stream for every command, and the child starts by
	 * emptying it: what this command read ahead but has not used yet is
	 * kept, and pushed back for it after (BSD's ungetc takes any amount). */
	size_t pending = stdin->_r > 0 ? (size_t)stdin->_r : 0;
	unsigned char *ahead = pending ? malloc(pending) : NULL;
	if (ahead != NULL) {
		memcpy(ahead, stdin->_p, pending);
	}
	int code = current.spawn(file, argv, &tty);
	fpurge(stdin);
	clearerr(stdin);
	for (size_t i = ahead ? pending : 0; i > 0; i--) {
		ungetc(ahead[i - 1], stdin);
	}
	free(ahead);
	return code;
}

int poopterm_actions_init(struct poopterm_actions *actions) {
	actions->count = 0;
	return 0;
}

int poopterm_actions_destroy(struct poopterm_actions *actions) {
	actions->count = 0;
	return 0;
}

static int add_action(struct poopterm_actions *actions, int kind, int fd, int from) {
	if (fd < 0 || actions->count == POOPTERM_ACTIONS) {
		return fd < 0 ? EBADF : ENOMEM;
	}
	actions->action[actions->count].kind = kind;
	actions->action[actions->count].fd = fd;
	actions->action[actions->count].from = from;
	actions->count++;
	return 0;
}

int poopterm_actions_open(struct poopterm_actions *actions, int fd, const char *path, int flags, mode_t mode) {
	if (strlen(path) >= sizeof actions->action[0].path) {
		return ENAMETOOLONG;
	}
	int failed = add_action(actions, POOPTERM_OPEN, fd, -1);
	if (failed == 0) {
		strcpy(actions->action[actions->count - 1].path, path);
		actions->action[actions->count - 1].flags = flags;
		actions->action[actions->count - 1].mode = mode;
	}
	return failed;
}

int poopterm_actions_dup2(struct poopterm_actions *actions, int from, int fd) {
	return add_action(actions, POOPTERM_DUP2, fd, from);
}

int poopterm_actions_close(struct poopterm_actions *actions, int fd) {
	return add_action(actions, POOPTERM_CLOSE, fd, -1);
}

int poopterm_spawn(pid_t *pid, const char *file, const struct poopterm_actions *actions, char *const argv[], char *const envp[]) {
	/* The child's standard descriptors, as the actions leave them; only
	 * those three reach it. */
	int fds[3] = {0, 1, 2};
	int opened[POOPTERM_ACTIONS];
	int open_count = 0, failed = 0;
	for (int i = 0; actions != NULL && i < actions->count && failed == 0; i++) {
		int fd = actions->action[i].fd;
		int from = actions->action[i].from;
		int made = -1;
		switch (actions->action[i].kind) {
		case POOPTERM_OPEN:
			made = open(actions->action[i].path, actions->action[i].flags | O_CLOEXEC, actions->action[i].mode);
			break;
		case POOPTERM_DUP2:
			made = from >= 0 && from <= 2 ? fds[from] : from;
			break;
		case POOPTERM_CLOSE:
			/* A closed standard descriptor reads and writes nothing. */
			made = open("/dev/null", O_RDWR | O_CLOEXEC);
			break;
		}
		if (made < 0) {
			failed = errno ? errno : EBADF;
			break;
		}
		if (actions->action[i].kind != POOPTERM_DUP2) {
			opened[open_count++] = made;
		}
		if (fd <= 2) {
			fds[fd] = made;
		}
	}
	int code = failed ? -failed : run_child(file, argv, envp, fds[0], fds[1], fds[2]);
	for (int i = 0; i < open_count; i++) {
		close(opened[i]);
	}
	if (code < 0) {
		return -code;
	}
	pid_t made = next_pid++;
	child_ended(made, code);
	if (pid != NULL) {
		*pid = made;
	}
	return 0;
}

#undef waitpid
pid_t poopterm_waitpid(pid_t pid, int *status_out, int options) {
	(void)options;
	for (int i = 0; i < ended_count; i++) {
		if (pid > 0 && ended[i].pid != pid) {
			continue;
		}
		pid_t found = ended[i].pid;
		if (status_out != NULL) {
			*status_out = ended[i].code << 8;
		}
		memmove(&ended[i], &ended[i + 1], sizeof ended[0] * (ended_count - i - 1));
		ended_count--;
		return found;
	}
	errno = ECHILD;
	return -1;
}

jmp_buf *poopterm_fork_begin(void) {
	fflush(NULL);
	forking.active = 1;
	forking.pid = next_pid++;
	for (int fd = 0; fd < 3; fd++) {
		forking.saved[fd] = fcntl(fd, F_DUPFD_CLOEXEC, 3);
	}
	forking.cwd = open(".", O_RDONLY | O_CLOEXEC);
	return &forking.back;
}

pid_t poopterm_forked(int pid) {
	return pid;
}

/* The child ended with `code`: puts back what it changed and returns from
 * `fork` again, as the parent. */
static void back_to_parent(int code) {
	fflush(NULL);
	child_ended(forking.pid, code);
	for (int fd = 0; fd < 3; fd++) {
		if (forking.saved[fd] >= 0) {
			dup2(forking.saved[fd], fd);
			close(forking.saved[fd]);
		}
	}
	if (forking.cwd >= 0) {
		fchdir(forking.cwd);
		close(forking.cwd);
	}
	forking.active = 0;
	longjmp(forking.back, forking.pid);
}

#undef execve
int poopterm_execve(const char *file, char *const argv[], char *const envp[]) {
	int code = run_child(file, argv, envp, 0, 1, 2);
	if (code < 0) {
		errno = -code;
		return -1;
	}
	/* The program ran in place of this one: this one ends with its status. */
	poopterm_exit(code);
}

#undef exit
int poopterm_entry(int argc, char **argv, const struct poopterm_tty *tty) {
	if (running) {
		dprintf(tty->err, "%s: cannot run inside itself\n", argc > 0 ? argv[0] : "command");
		return 126;
	}
	running = 1;
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
	ended_count = 0;
	forking.active = 0;
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
	running = 0;
	return status;
}
