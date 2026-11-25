/* ncdu's configuration on iOS and macOS, written by hand in place of the
 * one its configure script would generate. */
#define PACKAGE "ncdu"
#define PACKAGE_NAME "ncdu"
#define PACKAGE_VERSION "1.22"
#define PACKAGE_STRING "ncdu 1.22"
#define PACKAGE_TARNAME "ncdu"
#define PACKAGE_BUGREPORT "projects@yorhel.nl"
#define PACKAGE_URL "https://dev.yorhel.nl/ncdu"
#define VERSION "1.22"
#define DEFAULT_SHELL "/bin/sh"
#define FLEXIBLE_ARRAY_MEMBER /**/
#define STDC_HEADERS 1
#define HAVE_CHDIR 1
#define HAVE_DIRENT_H 1
#define HAVE_FNMATCH 1
#define HAVE_FNMATCH_H 1
#define HAVE_GETCWD 1
#define HAVE_GETENV 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIMITS_H 1
#define HAVE_LOCALE_H 1
#define HAVE_LSTAT 1
#define HAVE_NCURSES_H 1
#define HAVE_RMDIR 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRINGS_H 1
#define HAVE_STRING_H 1
#define HAVE_STRUCT_STAT_ST_BLOCKS 1
#define HAVE_ST_BLOCKS 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TIME_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_UNISTD_H 1
#define HAVE_UNLINK 1
#define HAVE_WCHAR_H 1
/* Firmlinks (getattrlist) exist on macOS only; iOS leaves them out. */
