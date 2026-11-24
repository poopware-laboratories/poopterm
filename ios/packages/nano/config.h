/*
 * nano's configuration for Poopterm's command libraries, written by hand
 * (a downloaded configure is never run): what Apple's libc and ncurses
 * have, and the features nano builds with.
 */
#ifndef POOPTERM_NANO_CONFIG_H
#define POOPTERM_NANO_CONFIG_H

#define PACKAGE "nano"
#define PACKAGE_NAME "GNU nano"
#define PACKAGE_TARNAME "nano"
#define PACKAGE_VERSION "8.7"
#define PACKAGE_STRING "GNU nano 8.7"
#define PACKAGE_BUGREPORT "nano@nano-editor.org"
#define PACKAGE_URL "https://nano-editor.org/"
#define VERSION "8.7"
#define LOCALEDIR "/usr/share/locale"
#define PKGDATADIR "/usr/share/nano"
#define SYSCONFDIR "/etc"

/* Features. */
#define ENABLE_BROWSER 1
#define ENABLE_COLOR 1
#define ENABLE_COMMENT 1
#define ENABLE_EXTRA 1
#define ENABLE_FORMATTER 1
#define ENABLE_HELP 1
#define ENABLE_HISTORIES 1
#define ENABLE_JUSTIFY 1
#define ENABLE_LINENUMBERS 1
#define ENABLE_LINTER 1
#define ENABLE_MOUSE 1
#define ENABLE_MULTIBUFFER 1
#define ENABLE_NANORC 1
#define ENABLE_OPERATINGDIR 1
#define ENABLE_TABCOMP 1
#define ENABLE_UTF8 1
#define ENABLE_WORDCOMPLETION 1
#define ENABLE_WRAPPING 1
/* No spell checker, formatter or linter runs: they start other programs
 * and wait, which a terminal that runs one program at a time cannot. */
/* #undef ENABLE_SPELLER */

/* Apple's libc and ncurses. */
#define HAVE_CURSES_H 1
#define HAVE_FLOCKFILE 1
#define HAVE_GETDELIM 1
#define HAVE_GETOPT_H 1
#define HAVE_GETOPT_LONG 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_GLOB 1
#define HAVE_GLOB_H 1
#define HAVE_ISWALNUM 1
#define HAVE_ISWALPHA 1
#define HAVE_ISWBLANK 1
#define HAVE_ISWCNTRL 1
#define HAVE_ISWCTYPE 1
#define HAVE_ISWPUNCT 1
#define HAVE_LANGINFO_CODESET 1
#define HAVE_LANGINFO_H 1
#define HAVE_MBRTOWC 1
#define HAVE_MBSTOWCS 1
#define HAVE_NL_LANGINFO 1
#define HAVE_SIGACTION 1
#define HAVE_STRCASECMP 1
#define HAVE_STRCASESTR 1
#define HAVE_STRNLEN 1
#define HAVE_SYS_PARAM_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TIME_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SYS_WAIT_H 1
#define HAVE_UNISTD_H 1
#define HAVE_USE_DEFAULT_COLORS 1
#define HAVE_WCHAR_H 1
#define HAVE_WCRTOMB 1
#define HAVE_WCWIDTH 1
#define HAVE_WINT_T 1
#define NANO_REG_EXTENDED REG_EXTENDED
#define STDC_HEADERS 1
#define _DARWIN_C_SOURCE 1

#endif
