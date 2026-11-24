/*
 * What the app gives the ssh server (`poopterm_sshd_serve`), and what the
 * server's patched session code calls back. Mirrored by src/inproc/sshd.rs.
 */
#ifndef POOPTERM_SSHD_H
#define POOPTERM_SSHD_H

struct poopterm_sshd {
	/* The one user, her home, and the password she may log in with ("" for
	 * keys only); the server's host key file. */
	const char *username;
	const char *home;
	const char *password;
	const char *hostkey;
	/* Starts a session: `cmd` is NULL for a login shell, `term` NULL
	 * without a pty. On success returns 0, the descriptors the server
	 * writes keys to and reads output from (both now the server's), and
	 * an id for the other calls. Otherwise minus an errno. */
	int (*open)(void *ctx, const char *cmd, const char *term, int cols, int rows,
		int *in_fd, int *out_fd, long *id);
	void (*resize)(void *ctx, long id, int cols, int rows);
	/* The exit status of a session that has ended, or -1. */
	int (*status)(void *ctx, long id);
	/* One line of the server's log. */
	void (*log)(void *ctx, const char *line);
	void *ctx;
};

/* Serves the ssh connection on `sock` until it ends, as the account of
 * `config`: 0 once it ended, or minus an errno for a server that could not
 * start (no host key). One connection at a time. */
int poopterm_sshd_serve(int sock, const struct poopterm_sshd *config);

#endif
