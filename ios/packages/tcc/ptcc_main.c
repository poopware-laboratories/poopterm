/*
 * Poopterm's tcc: the stock command line (tcc.c, with its main renamed
 * tcc_main), started with its library directory set from $TCCDIR, the
 * folder the app carries its headers in (include/, with tcc's own
 * stdarg.h and tccdefs.h beside ours).
 */
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libkern/OSCacheControl.h>

#include "libtcc.h"

/* tcc calls this after writing code; the iOS SDK's libSystem has no
 * __clear_cache, but the kernel's call does the same. */
void __clear_cache(void *begin, void *end) {
	sys_icache_invalidate(begin, (size_t)((char *)end - (char *)begin));
}

int tcc_main(int argc, char **argv);

/* What a program tcc runs must not do to the app it runs inside: ending it
 * (exit is tcc's own, which returns to where the program started; _exit
 * and _Exit are libc's, and go to the shim), starting other processes (nothing can), or asking the kernel
 * about a terminal it does not have. Its calls go to the command shim's
 * versions, which end only the command and answer for the device. */
static void aborted(void) {
	fflush(NULL);
	poopterm_exit(134);
}

static int refused(void) {
	errno = ENOSYS;
	return -1;
}

void poopterm_tcc_symbols(TCCState *state) {
	static const struct {
		const char *name;
		const void *function;
	} replaced[] = {
		{"_exit", (const void *)poopterm_exit},
		{"_Exit", (const void *)poopterm_exit},
		{"abort", (const void *)aborted},
		{"isatty", (const void *)poopterm_isatty},
		{"ioctl", (const void *)poopterm_ioctl},
		{"tcgetattr", (const void *)poopterm_tcgetattr},
		{"tcsetattr", (const void *)poopterm_tcsetattr},
		{"getpass", (const void *)poopterm_getpass},
		{"signal", (const void *)poopterm_signal},
		{"sigaction", (const void *)poopterm_sigaction},
		{"open", (const void *)poopterm_open},
		{"fopen", (const void *)poopterm_fopen},
		{"fork", (const void *)refused},
		{"vfork", (const void *)refused},
		{"system", (const void *)refused},
		{"popen", (const void *)refused},
		{"execv", (const void *)refused},
		{"execvp", (const void *)refused},
		{"execve", (const void *)refused},
		{"posix_spawn", (const void *)refused},
		{"posix_spawnp", (const void *)refused},
	};
	for (size_t i = 0; i < sizeof replaced / sizeof replaced[0]; i++) {
		tcc_add_symbol(state, replaced[i].name, replaced[i].function);
	}
}

int main(int argc, char **argv) {
	const char *home = getenv("TCCDIR");
	if (home == NULL || *home == '\0') {
		fprintf(stderr, "tcc: TCCDIR does not name the folder with its headers\n");
		return 1;
	}
	char **args = malloc(sizeof(char *) * (size_t)(argc + 3));
	size_t base = strlen(home) + 3;
	char *library = malloc(base);
	snprintf(library, base, "-B%s", home);
	int n = 0;
	args[n++] = argv[0];
	args[n++] = library;
	for (int i = 1; i < argc; i++) {
		args[n++] = argv[i];
	}
	args[n] = NULL;
	return tcc_main(n, args);
}
