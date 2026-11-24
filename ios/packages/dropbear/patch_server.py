"""Patches Dropbear's server sources for the app: sessions that run in the
app's shell, not a forked process on a pty, and one account the app names.

    python3 patch_server.py <dropbear src dir> <output dir>

Each change asserts that its text is there, so a new Dropbear release that
moves it fails the build instead of building something else."""
import re
import sys

src, out = sys.argv[1], sys.argv[2]


def read(name):
    with open(f"{src}/{name}") as file:
        return file.read()


def write(name, text):
    with open(f"{out}/{name}", "w") as file:
        file.write(text)


def replace(text, old, new, count=1):
    assert old in text, f"missing: {old[:60]!r}"
    return text.replace(old, new, count)


def between(text, start, end, new):
    """`text` with what lies from `start` up to (not including) `end` replaced."""
    first = text.index(start)
    last = text.index(end, first)
    return text[:first] + new + text[last:]


# Who may log in: the one account the app names, with the app's directory.
auth = read("svr-auth.c")
auth = replace(auth, "fill_passwd(username);", "poop_fill_passwd(username);")
auth = replace(auth, "\tsetusershell();\n", "\tgoto goodshell;\n\tsetusershell();\n")
auth = "void poop_fill_passwd(const char *username);\n" + auth
write("svr-auth.c", auth)

session = read("svr-chansession.c")
session = (
    """
extern int poop_open_session(const char *cmd, const char *term, int cols, int rows, int *in_fd, int *out_fd, long *id);
extern void poop_resize(long id, int cols, int rows);
extern int poop_session_status(long id);
#define POOP_CLOSED (-1)
/* The window the client asked for, and the session once it runs. */
static int poop_cols = 80, poop_rows = 24;
static long poop_id = -1;
"""
    + session
)

# A session is over when its output has ended; its status is the app's.
session = between(
    session,
    "static int sesscheckclose(struct Channel *channel) {",
    "/* Handler for childs exiting",
    """static int sesscheckclose(struct Channel *channel) {
	struct ChanSess *chansess = (struct ChanSess*)channel->typedata;

	if (chansess->pid != 0 && chansess->exit.exitpid == -1 && channel->readfd == POOP_CLOSED) {
		int status = poop_session_status(poop_id);
		chansess->exit.exitpid = chansess->pid;
		chansess->exit.exitstatus = status < 0 ? 0 : status;
		chansess->exit.exitsignal = -1;
	}
	if (chansess->exit.exitpid != -1) {
		channel->flushing = 1;
	}
	return chansess->pid == 0 || chansess->exit.exitpid != -1;
}

""",
)

# No child processes exist: no SIGCHLD handler (it would write to a pipe
# long closed), and no reaping of children that are the app's, not ours.
session = replace(
    session,
    """	sa_chld.sa_handler = sesssigchild_handler;
	sa_chld.sa_flags = SA_NOCLDSTOP;
	sigemptyset(&sa_chld.sa_mask);
	if (sigaction(SIGCHLD, &sa_chld, NULL) < 0) {
		dropbear_exit("signal() error");
	}
""",
    "",
)
session = replace(
    session,
    "\tif (!ses.channel_signal_pending) {\n\t\treturn;\n\t}\n\n\twhile ((pid = waitpid(-1, &status, WNOHANG)) > 0) {",
    "\tif (!ses.channel_signal_pending) {\n\t\treturn;\n\t}\n\n\twhile (0) {",
)

# A pty is promised, not made: the app's terminal has the window.
session = between(
    session,
    "\t/* allocate the pty */",
    "\tTRACE((\"leave sessionpty\"))",
    """\tif (chansess->master != -1) {
		dropbear_exit("Multiple pty requests");
	}
	chansess->master = 0;
	sessionwinchange(chansess);

""",
)
session = replace(
    session,
    "\tpty_change_window_size(chansess->master, termr, termc, termw, termh);",
    "\tpoop_cols = termc;\n\tpoop_rows = termr;\n\tif (poop_id >= 0) {\n\t\tpoop_resize(poop_id, termc, termr);\n\t}",
)

# Starting the shell or command: the app opens it.
session = between(
    session,
    "static int noptycommand(struct Channel *channel, struct ChanSess *chansess) {",
    "static void addchildpid(",
    """static int poopcommand(struct Channel *channel, struct ChanSess *chansess) {
	int in_fd = -1, out_fd = -1;
	long id = -1;

	if (poop_open_session(chansess->cmd, chansess->term, poop_cols, poop_rows,
			&in_fd, &out_fd, &id) != 0) {
		return DROPBEAR_FAILURE;
	}
	setnonblocking(in_fd);
	setnonblocking(out_fd);
	channel->writefd = in_fd;
	channel->readfd = out_fd;
	channel->errfd = POOP_CLOSED;
	channel->bidir_fd = 0;
	ses.maxfd = MAX(ses.maxfd, MAX(in_fd, out_fd));
	poop_id = id;
	chansess->pid = (pid_t)(1000 + id);
	return DROPBEAR_SUCCESS;
}

static int noptycommand(struct Channel *channel, struct ChanSess *chansess) {
	return poopcommand(channel, chansess);
}

static int ptycommand(struct Channel *channel, struct ChanSess *chansess) {
	return poopcommand(channel, chansess);
}

""",
)
write("svr-chansession.c", session)
