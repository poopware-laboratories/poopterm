/*
 * GNU make's configuration for Poopterm's command libraries, written by
 * hand (configure is never run): Darwin's libc, recipes started with
 * vfork and exec (which the command shim runs in this process), one job
 * at a time (no jobserver), and /bin/sh -c answered by the shell.
 */
#define PACKAGE "make"
#define PACKAGE_NAME "GNU make"
#define PACKAGE_TARNAME "make"
#define PACKAGE_VERSION "4.4.1"
#define PACKAGE_STRING "GNU make 4.4.1"
#define PACKAGE_BUGREPORT "bug-make@gnu.org"
#define PACKAGE_URL "https://www.gnu.org/software/make/"
#define VERSION "4.4.1"
#define MAKE_HOST "arm64-apple-darwin"
#define MAKE_SYMLINKS 1
#define PATH_SEPARATOR_CHAR ':'
#define FILE_TIMESTAMP_HI_RES 1
#define ST_MTIM_NSEC st_mtimespec.tv_nsec
#define STDC_HEADERS 1

#define HAVE_ALLOCA 1
#define HAVE_ALLOCA_H 1
#define HAVE_ATEXIT 1
#define HAVE_CASE_INSENSITIVE_FS 1
#define HAVE_CLOCK_GETTIME 1
#define HAVE_DIRENT_H 1
#define HAVE_DUP 1
#define HAVE_DUP2 1
#define HAVE_FCNTL_H 1
#define HAVE_FDOPEN 1
#define HAVE_FORK 1
#define HAVE_GETCWD 1
#define HAVE_GETGROUPS 1
#define HAVE_GETHOSTNAME 1
#define HAVE_GETRLIMIT 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_INTMAX_T 1
#define HAVE_INTTYPES_H 1
#define HAVE_ISATTY 1
#define HAVE_LIMITS_H 1
#define HAVE_LOCALE_H 1
#define HAVE_LONG_LONG_INT 1
#define HAVE_LSTAT 1
#define HAVE_MKFIFO 1
#define HAVE_MKSTEMP 1
#define HAVE_MKTEMP 1
#define HAVE_PIPE 1
#define HAVE_READLINK 1
#define HAVE_REALPATH 1
#define HAVE_SA_RESTART 1
#define HAVE_SETEGID 1
#define HAVE_SETEUID 1
#define HAVE_SETLINEBUF 1
#define HAVE_SETREGID 1
#define HAVE_SETREUID 1
#define HAVE_SETRLIMIT 1
#define HAVE_SETVBUF 1
#define HAVE_SIGACTION 1
#define HAVE_SIG_ATOMIC_T 1
#define HAVE_STDBOOL_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STPCPY 1
#define HAVE_STRCASECMP 1
#define HAVE_STRCOLL 1
#define HAVE_STRDUP 1
#define HAVE_STRERROR 1
#define HAVE_STRINGS_H 1
#define HAVE_STRING_H 1
#define HAVE_STRNCASECMP 1
#define HAVE_STRNDUP 1
#define HAVE_STRSIGNAL 1
#define HAVE_STRTOLL 1
#define HAVE_STRUCT_DIRENT_D_TYPE 1
#define HAVE_SYS_FILE_H 1
#define HAVE_SYS_PARAM_H 1
#define HAVE_SYS_RESOURCE_H 1
#define HAVE_SYS_SELECT_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TIME_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SYS_WAIT_H 1
#define HAVE_TTYNAME 1
#define HAVE_UINTMAX_T 1
#define HAVE_UMASK 1
#define HAVE_UNISTD_H 1
#define HAVE_UNSIGNED_LONG_LONG_INT 1
#define HAVE_WAITPID 1
#define HAVE_WCHAR_H 1
#define HAVE_WORKING_FORK 1
#define HAVE_DECL_BSD_SIGNAL 1
#define HAVE_DECL_DLERROR 0
#define HAVE_DECL_DLOPEN 0
#define HAVE_DECL_DLSYM 0
#define HAVE_DECL_GETLOADAVG 1
#define HAVE_DECL_SYS_SIGLIST 0
#define HAVE_DECL__SYS_SIGLIST 0
#define HAVE_DECL___SYS_SIGLIST 0

/* Darwin has no mempcpy: src/misc.c defines one (HAVE_MEMPCPY is unset),
 * and the other files need its prototype. */
extern void *mempcpy (void *to, const void *from, unsigned long size);
/* The SCCS "get" its implicit rules name. */
#define SCCS_GET "get"

/* iOS has no <ar.h>: archive members (lib.a(member.o)) are not supported. */
#define NO_ARCHIVES 1
