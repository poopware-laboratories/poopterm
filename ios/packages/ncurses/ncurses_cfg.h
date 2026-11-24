/*
 * ncurses' configuration for Poopterm, written by hand (configure is
 * never run): a wide-character build on Darwin, reading the terminal
 * descriptions from files (TERMINFO, which the app points at the ones it
 * carries), and the terminal calls (tcgetattr, ioctl, isatty) going to
 * Poopterm's device through the command shim included before every file.
 */
#ifndef NC_CONFIG_H
#define NC_CONFIG_H

#define PACKAGE "ncurses"
#define NCURSES_VERSION_STRING "6.5.20240427"
#define SYSTEM_NAME "darwin"

#define TERMINFO "/usr/share/terminfo"
#define TERMINFO_DIRS "/usr/share/terminfo"
#define TERMPATH "/etc/termcap:/usr/share/misc/termcap"
#define NCURSES_PATHSEP ':'
#define NCURSES_PATH_SEP ':'
#define USE_HOME_TERMINFO 1
#define USE_ROOT_ENVIRON 1
#define USE_SYMLINKS 1
#define USE_SIGWINCH 1
#define USE_HASHMAP 1
#define USE_WIDEC_SUPPORT 1
#define USE_SAFE_SPRINTF 1
#define USE_COLORFGBG 1
#define USE_ASSUMED_COLOR 1
#define USE_ITALIC 1
#define USE_XMC_SUPPORT 0
#define USE_HARD_TABS 1
#define USE_SIGTSTP 1
#define USE_SIZECHANGE 1
#define NCURSES_EXT_FUNCS 1
#define NCURSES_EXT_COLORS 1
#define HAVE_INIT_EXTENDED_COLOR 1
#define NCURSES_XNAMES 1
#define EXP_XTERM_1005 1

#define HAVE_BTOWC 1
#define HAVE_CLOCK_GETTIME 1
#define HAVE_FCNTL_H 1
#define HAVE_FPATHCONF 1
#define HAVE_GETCWD 1
#define HAVE_GETEGID 1
#define HAVE_GETEUID 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_GETUID 1
#define HAVE_INTTYPES_H 1
#define HAVE_ISSETUGID 1
#define HAVE_LANGINFO_CODESET 1
#define HAVE_LIMITS_H 1
#define HAVE_LINK 1
#define HAVE_LOCALE_H 1
#define HAVE_LONG_FILE_NAMES 1
#define HAVE_MBLEN 1
#define HAVE_MBRLEN 1
#define HAVE_MBRTOWC 1
#define HAVE_MBTOWC 1
#define HAVE_NANOSLEEP 1
#define HAVE_POLL_H 1
#define HAVE_PUTENV 1
#define HAVE_REMOVE 1
#define HAVE_RESIZETERM 1
#define HAVE_SELECT 1
#define HAVE_SETBUF 1
#define HAVE_SETENV 1
#define HAVE_SETVBUF 1
#define HAVE_SIGACTION 1
#define HAVE_SIZECHANGE 1
#define HAVE_SNPRINTF 1
#define HAVE_STDINT_H 1
#define HAVE_STRDUP 1
#define HAVE_STRLCAT 1
#define HAVE_STRLCPY 1
#define HAVE_STRSTR 1
#define HAVE_SYSCONF 1
#define HAVE_SYS_IOCTL_H 1
#define HAVE_SYS_PARAM_H 1
#define HAVE_SYS_SELECT_H 1
#define HAVE_SYS_TIMES_H 1
#define HAVE_SYS_TIME_H 1
#define HAVE_SYS_TIME_SELECT 1
#define HAVE_TCGETATTR 1
#define HAVE_TCGETPGRP 1
#define HAVE_TERMIOS_H 1
#define HAVE_TIMES 1
#define HAVE_TSEARCH 1
#define HAVE_TYPE_SIGACTION 0
#define HAVE_UNISTD_H 1
#define HAVE_VA_COPY 1
#define HAVE_VSNPRINTF 1
#define HAVE_VSSCANF 1
#define HAVE_WCTOB 1
#define HAVE_WCTOMB 1
#define HAVE_WCTYPE_H 1
#define HAVE_WMEMCHR 1
#define HAVE_CONSISTENT_GETENV 1
#define MIXEDCASE_FILENAMES 1
#define SIZEOF_WCHAR_T 4
#define SIZEOF_SHORT 2
#define STDC_HEADERS 1
#define SIG_ATOMIC_T volatile sig_atomic_t
#define GCC_PRINTF 1
#define GCC_SCANF 1
#define GCC_NORETURN __attribute__((noreturn))
#define GCC_UNUSED __attribute__((unused))
#define GCC_PRINTFLIKE(fmt,var) __attribute__((format(printf,fmt,var)))
#define GCC_SCANFLIKE(fmt,var) __attribute__((format(scanf,fmt,var)))
#define GCC_DEPRECATED(msg) /* nothing */

#endif
