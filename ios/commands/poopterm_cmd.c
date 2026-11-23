/*
 * Linked into every command library: the entry Poopterm calls. It points
 * the standard descriptors at the tab's device for the command's run,
 * resets the state commands assume a fresh process has (getopt, stdio),
 * and turns `exit` into a return.
 */
#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <pthread.h>
#include <setjmp.h>
#include <signal.h>
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

/* The C library's exit, for calls the header's macro does not catch (git
 * defines its own exit macro, calling the function by name): within this
 * library the name binds here, so it ends the command, not the app. */
#undef exit
void exit(int code) {
	poopterm_exit(code);
}

void poopterm_errx(int code, const char *format, ...) {
	va_list args;
	va_start(args, format);
	vwarnx(format, args);
	va_end(args);
	poopterm_exit(code);
}

/* Descriptors this run opened /dev/tty as. */
#define POOPTERM_TTY_OPENS 8
static int tty_opens[POOPTERM_TTY_OPENS];

/* Whether `fd` is the terminal: a standard descriptor that is, or one
 * /dev/tty was opened as. */
static int is_terminal(int fd) {
	if (fd >= 0 && fd <= 2) {
		return current.tty_fds >> fd & 1;
	}
	for (int i = 0; fd >= 0 && i < POOPTERM_TTY_OPENS; i++) {
		if (tty_opens[i] == fd) {
			return 1;
		}
	}
	return 0;
}

#undef isatty
int poopterm_isatty(int fd) {
	if (fd >= 0 && fd <= 2 || is_terminal(fd)) {
		return is_terminal(fd);
	}
	return isatty(fd);
}

#undef open
int poopterm_open(const char *path, int flags, ...) {
	va_list args;
	va_start(args, flags);
	mode_t mode = (flags & O_CREAT) ? (mode_t)va_arg(args, int) : 0;
	va_end(args);
	if (path == NULL || strcmp(path, "/dev/tty") != 0) {
		return open(path, flags, mode);
	}
	if (current.keyboard < 0) {
		errno = ENXIO;
		return -1;
	}
	int fd = fcntl(current.keyboard, (flags & O_CLOEXEC) ? F_DUPFD_CLOEXEC : F_DUPFD, 3);
	for (int i = 0; fd >= 0 && i < POOPTERM_TTY_OPENS; i++) {
		if (tty_opens[i] < 0) {
			tty_opens[i] = fd;
			break;
		}
	}
	return fd;
}

/* The POOPTERM_TTY_* operation an ioctl request is, or 0. */
static int tty_operation(unsigned long request) {
	switch (request) {
	case TIOCGETA: return POOPTERM_TTY_GET_SETTINGS;
	case TIOCSETA: return POOPTERM_TTY_SET_SETTINGS;
	case TIOCSETAW: return POOPTERM_TTY_SET_SETTINGS_DRAIN;
	case TIOCSETAF: return POOPTERM_TTY_SET_SETTINGS_FLUSH;
	case TIOCGWINSZ: return POOPTERM_TTY_GET_SIZE;
	default: return 0;
	}
}

#undef ioctl
int poopterm_ioctl(int fd, unsigned long request, ...) {
	va_list args;
	va_start(args, request);
	void *arg = va_arg(args, void *);
	va_end(args);
	int op = tty_operation(request);
	/* The device has one line discipline, the standard one (stty asks). */
	if ((request == TIOCGETD || request == TIOCSETD) && is_terminal(fd)) {
		if (request == TIOCGETD) {
			*(int *)arg = TTYDISC;
		}
		return 0;
	}
	if (op != 0 && (fd >= 0 && fd <= 2 || is_terminal(fd))) {
		if (!is_terminal(fd)) {
			errno = ENOTTY;
			return -1;
		}
		if (current.control != NULL) {
			int failed = current.control(current.context, op, arg);
			if (failed < 0) {
				errno = -failed;
				return -1;
			}
			return 0;
		}
		if (op != POOPTERM_TTY_GET_SIZE) {
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

#undef tcgetattr
int poopterm_tcgetattr(int fd, struct termios *settings) {
	return poopterm_ioctl(fd, TIOCGETA, settings);
}

#undef tcsetattr
int poopterm_tcsetattr(int fd, int when, const struct termios *settings) {
	switch (when & ~TCSASOFT) {
	case TCSANOW: return poopterm_ioctl(fd, TIOCSETA, settings);
	case TCSADRAIN: return poopterm_ioctl(fd, TIOCSETAW, settings);
	case TCSAFLUSH: return poopterm_ioctl(fd, TIOCSETAF, settings);
	default:
		errno = EINVAL;
		return -1;
	}
}

/* Signals a command may handle, given back to the app's handlers when it
 * ends. Those that by default end a process end the command; those that
 * stop one do nothing. */
static const int ending_signals[] = {SIGINT, SIGQUIT, SIGHUP, SIGTERM, SIGALRM, SIGPIPE, SIGUSR1, SIGUSR2, SIGVTALRM, SIGPROF, SIGXCPU};
static const int stopping_signals[] = {SIGTSTP, SIGTTIN, SIGTTOU};
static const int other_signals[] = {SIGWINCH, SIGCHLD, SIGCONT, SIGINFO, SIGURG};
#define COUNT(list) (int)(sizeof list / sizeof list[0])
static struct sigaction app_actions[NSIG];
static pthread_t command_thread;

static int listed(int sig, const int *list, int count) {
	for (int i = 0; i < count; i++) {
		if (list[i] == sig) {
			return 1;
		}
	}
	return 0;
}

/* SIGINT and the like, on the command's thread, end the command. */
static void ended_by(int sig) {
	if (running && pthread_equal(pthread_self(), command_thread)) {
		poopterm_exit(128 + sig);
	}
}

static void stopped_by(int sig) {
	(void)sig;
}

/* What a handler means here: the default of an ending or stopping signal
 * becomes ours, so the app is never ended or stopped. */
static void (*meaning(int sig, void (*handler)(int)))(int) {
	if (handler != SIG_DFL) {
		return handler;
	}
	if (listed(sig, ending_signals, COUNT(ending_signals))) {
		return ended_by;
	}
	if (listed(sig, stopping_signals, COUNT(stopping_signals))) {
		return stopped_by;
	}
	return handler;
}

/* And back: ours read as the default the command asked for. */
static void (*shown(void (*handler)(int)))(int) {
	return handler == ended_by || handler == stopped_by ? SIG_DFL : handler;
}

#undef sigaction
int poopterm_sigaction(int sig, const struct sigaction *action, struct sigaction *old) {
	struct sigaction mine;
	if (action != NULL) {
		mine = *action;
		if (!(mine.sa_flags & SA_SIGINFO)) {
			mine.sa_handler = meaning(sig, mine.sa_handler);
		}
		action = &mine;
	}
	int failed = sigaction(sig, action, old);
	if (failed == 0 && old != NULL && !(old->sa_flags & SA_SIGINFO)) {
		old->sa_handler = shown(old->sa_handler);
	}
	return failed;
}

#undef signal
void (*poopterm_signal(int sig, void (*handler)(int)))(int) {
	struct sigaction action, old;
	memset(&action, 0, sizeof action);
	action.sa_handler = handler;
	/* BSD's signal() restarts interrupted calls. */
	action.sa_flags = SA_RESTART;
	sigemptyset(&action.sa_mask);
	if (poopterm_sigaction(sig, &action, &old) != 0) {
		return SIG_ERR;
	}
	return old.sa_handler;
}

/* The thread's signal mask before the command, put back after: a command
 * may leave signals blocked (pax does), which would keep ^C from every
 * command after it. */
static sigset_t app_mask;

/* Saves the app's handlers and puts the command's defaults in place, with
 * those signals unblocked. */
static void take_signals(void) {
	command_thread = pthread_self();
	sigset_t unblock;
	sigemptyset(&unblock);
	for (int i = 0; i < COUNT(ending_signals); i++) {
		sigaddset(&unblock, ending_signals[i]);
	}
	for (int i = 0; i < COUNT(stopping_signals); i++) {
		sigaddset(&unblock, stopping_signals[i]);
	}
	pthread_sigmask(SIG_UNBLOCK, &unblock, &app_mask);
	const int *lists[] = {ending_signals, stopping_signals, other_signals};
	const int counts[] = {COUNT(ending_signals), COUNT(stopping_signals), COUNT(other_signals)};
	for (int list = 0; list < 3; list++) {
		for (int i = 0; i < counts[list]; i++) {
			int sig = lists[list][i];
			sigaction(sig, NULL, &app_actions[sig]);
			if (list < 2) {
				struct sigaction action;
				memset(&action, 0, sizeof action);
				action.sa_handler = list == 0 ? ended_by : stopped_by;
				sigemptyset(&action.sa_mask);
				sigaction(sig, &action, NULL);
			}
		}
	}
}

/* Gives the app its handlers and signal mask back. */
static void give_back_signals(void) {
	pthread_sigmask(SIG_SETMASK, &app_mask, NULL);
	const int *lists[] = {ending_signals, stopping_signals, other_signals};
	const int counts[] = {COUNT(ending_signals), COUNT(stopping_signals), COUNT(other_signals)};
	for (int list = 0; list < 3; list++) {
		for (int i = 0; i < counts[list]; i++) {
			int sig = lists[list][i];
			sigaction(sig, &app_actions[sig], NULL);
		}
	}
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
	/* The child's descriptor n is the terminal when it is one of ours that is. */
	const int to[3] = {in, out, err};
	tty.tty_fds = 0;
	for (int fd = 0; fd < 3; fd++) {
		if (is_terminal(to[fd])) {
			tty.tty_fds |= 1u << fd;
		}
	}
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

/* Gives the standard streams buffers of stdio's own again: a command may
 * have pointed one at a buffer of its own (setvbuf with a static array),
 * gone once its library is unloaded, where the next command would read
 * or write. Output to the terminal is line-buffered, so a prompt shows
 * before the program waits for keys, and standard error is unbuffered. */
static void fresh_buffers(void) {
	fpurge(stdin);
	clearerr(stdin);
	setvbuf(stdin, NULL, _IOFBF, 0);
	setvbuf(stdout, NULL, is_terminal(1) ? _IOLBF : _IOFBF, 0);
	setvbuf(stderr, NULL, _IONBF, 0);
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
	fresh_buffers();
	clearerr(stdout);
	clearerr(stderr);
	ended_count = 0;
	forking.active = 0;
	for (int i = 0; i < POOPTERM_TTY_OPENS; i++) {
		tty_opens[i] = -1;
	}
	take_signals();
	optind = 1;
	optreset = 1;
	opterr = 1;
	status = 0;
	if (setjmp(finished) == 0) {
		status = poopterm_main(argc, argv);
	}
	give_back_signals();
	fflush(stdout);
	fflush(stderr);
	fresh_buffers();
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
