/*
 * tcc's configuration for Poopterm, written by hand (configure is never
 * run): the arm64 Mach-O target, running code in memory only, reading its
 * headers (tccdefs.h included) from the directory TCCDIR names at run time.
 */
#define TCC_VERSION "0.9.28rc"
#define TCC_TARGET_ARM64 1
#define TCC_TARGET_MACHO 1
#define CONFIG_TCC_PREDEFS 0
#define ONE_SOURCE 1
#define CONFIG_TCCDIR "/usr/local/lib/tcc"
#define CONFIG_TCC_SYSINCLUDEPATHS "{B}/include"
#define CONFIG_TCC_LIBPATHS "{B}"
#define CONFIG_TCC_CRTPREFIX "{B}"
#define CONFIG_TCC_ELFINTERP ""
