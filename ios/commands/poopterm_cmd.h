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
#include <stdarg.h>
/* A command that names something `err` itself (pr, git) leaves err.h
 * out, and gets only the warn family. */
#ifndef POOPTERM_NO_ERR_H
#include <err.h>
#else
void warn(const char *, ...);
void warnx(const char *, ...);
void warnc(int, const char *, ...);
void vwarn(const char *, va_list);
void vwarnx(const char *, va_list);
#endif
#include <fcntl.h>
#include <setjmp.h>
#include <signal.h>
#include <spawn.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

/* What the shell passes a command: its standard descriptors and terminal. */
struct poopterm_tty {
	int in, out, err;
	/* Whether the descriptors are the tab's terminal (not a file or pipe). */
	int is_tty;
	unsigned short cols, rows;
	/* The command's environment, `NAME=value` strings ending in NULL. */
	char **envp;
	/* Runs the command `file` names (found on the PATH of `tty->envp`)
	 * with `argv` on `tty`'s descriptors, and returns its exit status, or
	 * minus an errno: ENOENT when there is none, ENOEXEC when it is no
	 * command. The shell provides it. */
	int (*spawn)(const char *file, char *const *argv, const struct poopterm_tty *tty);
	/* The terminal, when there is one: `control` does a POOPTERM_TTY_*
	 * operation on it (`context` passed back) and returns 0 or minus an
	 * errno. The device keeps the settings programs set, as a kernel would. */
	void *context;
	int (*control)(void *context, int op, void *arg);
	/* Bit n set: descriptor n (0 to 2) is the terminal. */
	unsigned tty_fds;
	/* Where the keyboard reads, which opening /dev/tty gives, or -1. */
	int keyboard;
};

#define POOPTERM_TTY_GET_SETTINGS 1 /* struct termios * */
#define POOPTERM_TTY_SET_SETTINGS 2
#define POOPTERM_TTY_SET_SETTINGS_DRAIN 3
#define POOPTERM_TTY_SET_SETTINGS_FLUSH 4
#define POOPTERM_TTY_GET_SIZE 5 /* struct winsize * */

int poopterm_entry(int argc, char **argv, const struct poopterm_tty *tty);
void poopterm_exit(int status) __attribute__((noreturn));
int poopterm_isatty(int fd);
int poopterm_ioctl(int fd, unsigned long request, ...);
int poopterm_fclose(FILE *stream);
int poopterm_open(const char *path, int flags, ...);
FILE *poopterm_fopen(const char *path, const char *mode);
char *poopterm_getpass(const char *prompt);
int poopterm_tcgetattr(int fd, struct termios *settings);
int poopterm_tcsetattr(int fd, int when, const struct termios *settings);
void (*poopterm_signal(int sig, void (*handler)(int)))(int);
int poopterm_sigaction(int sig, const struct sigaction *action, struct sigaction *old);
void poopterm_errx(int status, const char *format, ...) __attribute__((noreturn));

#define exit(status) poopterm_exit(status)
#define _exit(status) poopterm_exit(status)
#ifndef POOPTERM_NO_ERR_H
#define err(status, ...) (warn(__VA_ARGS__), poopterm_exit(status))
#define errx(status, ...) (warnx(__VA_ARGS__), poopterm_exit(status))
#define errc(status, code, ...) (warnc(code, __VA_ARGS__), poopterm_exit(status))
#define verr(status, format, args) (vwarn(format, args), poopterm_exit(status))
#define verrx(status, format, args) (vwarnx(format, args), poopterm_exit(status))
#else
/* Without err.h, `err` may be the command's own name (pr's stream, git's
 * function), and errx a function the command declares itself. */
#define errx poopterm_errx
#endif
#define isatty(fd) poopterm_isatty(fd)
#define ioctl(...) poopterm_ioctl(__VA_ARGS__)
/* Closing a standard stream (cat and others do at the end) only flushes
 * it: the streams are the app's, and the next command needs them. */
#define fclose(stream) poopterm_fclose(stream)
/* A program that reads keys from /dev/tty (less, vim) is built with
 * POOPTERM_DEV_TTY: there is no such device, so opening it gives the
 * keyboard. Not for all, since `open` is also a common member name. */
#ifdef POOPTERM_DEV_TTY
#define open(...) poopterm_open(__VA_ARGS__)
#define fopen(path, mode) poopterm_fopen(path, mode)
#endif
/* libc's getpass opens /dev/tty itself: this one asks on the terminal
 * device, with echo off, and reads from its keyboard. */
#define getpass(prompt) poopterm_getpass(prompt)
/* The terminal's settings are the device's, not a kernel's. */
#define tcgetattr(fd, settings) poopterm_tcgetattr(fd, settings)
#define tcsetattr(fd, when, settings) poopterm_tcsetattr(fd, when, settings)
/* A signal whose default would end or stop the process ends the command
 * instead (`^C` is SIGINT on its thread); handlers a command installs are
 * taken back when it ends, since its library may then be unloaded. */
#define signal(sig, handler) poopterm_signal(sig, handler)
#define sigaction(sig, action, old) poopterm_sigaction(sig, action, old)
/*
 * No other process may start, so a command that starts one runs it in
 * this one instead, through the shell's `spawn`, before carrying on:
 * `posix_spawn` with its file actions (xargs), and `fork` then `exec`
 * (find -exec), where `fork` returns 0 to run the child's code, and the
 * child's `exec` or `_exit` comes back to `fork`'s caller as the parent,
 * with the descriptors and directory the child changed put back. Each
 * child has ended by the time it is started, so `waitpid` answers at once.
 */
#define POOPTERM_ACTIONS 8
struct poopterm_actions {
	int count;
	struct {
		enum { POOPTERM_OPEN, POOPTERM_DUP2, POOPTERM_CLOSE } kind;
		int fd, from, flags;
		mode_t mode;
		char path[1024];
	} action[POOPTERM_ACTIONS];
};
int poopterm_actions_init(struct poopterm_actions *actions);
int poopterm_actions_destroy(struct poopterm_actions *actions);
int poopterm_actions_open(struct poopterm_actions *actions, int fd, const char *path, int flags, mode_t mode);
int poopterm_actions_dup2(struct poopterm_actions *actions, int from, int fd);
int poopterm_actions_close(struct poopterm_actions *actions, int fd);
int poopterm_spawn(pid_t *pid, const char *file, const struct poopterm_actions *actions, char *const argv[], char *const envp[]);
pid_t poopterm_waitpid(pid_t pid, int *status, int options);
jmp_buf *poopterm_fork_begin(void);
pid_t poopterm_forked(int pid);
int poopterm_execve(const char *file, char *const argv[], char *const envp[]);

#define posix_spawn_file_actions_t struct poopterm_actions
#define posix_spawn_file_actions_init(actions) poopterm_actions_init(actions)
#define posix_spawn_file_actions_destroy(actions) poopterm_actions_destroy(actions)
#define posix_spawn_file_actions_addopen(actions, fd, path, flags, mode) \
	poopterm_actions_open(actions, fd, path, flags, mode)
#define posix_spawn_file_actions_adddup2(actions, from, fd) poopterm_actions_dup2(actions, from, fd)
#define posix_spawn_file_actions_addclose(actions, fd) poopterm_actions_close(actions, fd)
#define posix_spawn(pid, path, actions, attr, argv, envp) poopterm_spawn(pid, path, actions, argv, envp)
#define posix_spawnp(pid, file, actions, attr, argv, envp) poopterm_spawn(pid, file, actions, argv, envp)
#define waitpid(pid, status, options) poopterm_waitpid(pid, status, options)
/* setjmp is called in the caller's frame, which is still running when the
 * child's exec comes back to it. */
#define fork() poopterm_forked(setjmp(*poopterm_fork_begin()))
#define vfork() fork()
#define execvp(file, argv) poopterm_execve(file, argv, NULL)
#define execv(path, argv) poopterm_execve(path, argv, NULL)
#define execve(path, argv, envp) poopterm_execve(path, argv, envp)

/* The command's own `main`, called by `poopterm_entry`. A `main` that
 * relies on C's implicit `return 0` keeps its name (POOPTERM_KEEP_MAIN)
 * and is linked as `poopterm_main` with `-Wl,-alias,_main,_poopterm_main`. */
#ifndef POOPTERM_KEEP_MAIN
#define main poopterm_main
#endif

#endif
