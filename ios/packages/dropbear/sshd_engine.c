/*
 * The ssh server's entry for the app: Dropbear's own protocol, keys and
 * authentication serving one connection a call, in place of its main
 * (svr-main.c, which forks). The patched sessions (patch_server.py) ask
 * the app for their shell through `poopterm_sshd`.
 */
#include "includes.h"
#include "dbutil.h"
#include "session.h"
#include "buffer.h"
#include "signkey.h"
#include "runopts.h"
#include "dbrandom.h"
#include "crypto_desc.h"
#include "auth.h"
#include <setjmp.h>
#include <unistd.h>
#include "sshd_engine.h"

static const struct poopterm_sshd *config;
static jmp_buf ended;
static int ending;
static int started;
/* Whether the connection is the session's now (it closes it). */
static int sock_taken;

void poop_exit(int code) {
	(void)code;
	if (!ending) {
		/* Nothing to go back to: never happens in a served connection. */
		_exit(code);
	}
	longjmp(ended, 1);
}

/* The server's log lines, to the app instead of stderr or syslog. */
static void poop_log(int priority, const char *format, va_list param) {
	char line[512];
	(void)priority;
	vsnprintf(line, sizeof line, format, param);
	if (config && config->log) {
		config->log(config->ctx, line);
	}
}

void poop_fill_passwd(const char *username) {
	char *hash;
	if (!config || strcmp(username, config->username) != 0) {
		return;
	}
	ses.authstate.pw_uid = geteuid();
	ses.authstate.pw_gid = getegid();
	ses.authstate.pw_name = m_strdup(config->username);
	ses.authstate.pw_dir = m_strdup(config->home);
	ses.authstate.pw_shell = m_strdup("/bin/sh");
	hash = config->password[0] ? crypt(config->password, "pt") : NULL;
	/* No password: a hash nothing matches, which refuses every attempt. */
	ses.authstate.pw_passwd = m_strdup(hash ? hash : "!");
}

/* The session code's calls. One session at a time. */
int poop_open_session(const char *cmd, const char *term, int cols, int rows, int *in_fd, int *out_fd, long *id) {
	return config->open(config->ctx, cmd, term, cols, rows, in_fd, out_fd, id);
}

void poop_resize(long id, int cols, int rows) {
	config->resize(config->ctx, id, cols, rows);
}

int poop_session_status(long id) {
	return config->status(config->ctx, id);
}

int poopterm_sshd_serve(int sock, const struct poopterm_sshd *given) {
	config = given;
	sock_taken = 0;
	/* Whatever ends the server through `exit`, setting up or serving,
	 * comes back here. */
	ending = 1;
	if (setjmp(ended) != 0) {
		ending = 0;
		/* Before the session began, the options or keys were refused. */
		return sock_taken ? 0 : -EINVAL;
	}
	int again = started;
	if (!started) {
		char *argv[] = {"sshd", "-i", "-k", "-r", (char *)given->hostkey, NULL};
		_dropbear_exit = svr_dropbear_exit;
		_dropbear_log = poop_log;
		if (access(given->hostkey, R_OK) != 0) {
			ending = 0;
			return -ENOENT;
		}
		svr_getopts(5, argv);
		crypto_init();
		seedrandom();
		started = 1;
	}
	/* A session frees the host key when it ends, and loading it frees
	 * the file name: give it its name again, and read it. */
	if (again) {
		svr_opts.hostkey_files[0] = m_strdup(given->hostkey);
		svr_opts.num_hostkey_files = 1;
	}
	load_all_hostkeys();
	/* Dropbear serves from a fresh process: its session state starts zeroed. */
	memset(&ses, 0, sizeof ses);
	memset(&svr_ses, 0, sizeof svr_ses);
	sock_taken = 1;
	svr_session(sock, -1);
	ending = 0;
	return 0;
}
