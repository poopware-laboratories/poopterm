/*
 * Dropbear's configuration for Poopterm's command libraries, written by
 * hand (configure is never run): the ssh client on Darwin, with the bundled
 * libtomcrypt and libtommath, no server and no login records.
 */
#ifndef POOPTERM_DROPBEAR_CONFIG_H
#define POOPTERM_DROPBEAR_CONFIG_H

#define PACKAGE_NAME "Dropbear"
#define PACKAGE_TARNAME "dropbear"
#define PACKAGE_VERSION "2026.94"
#define PACKAGE_STRING "Dropbear 2026.94"
#define PACKAGE_BUGREPORT ""
#define PACKAGE_URL ""

#define BUNDLED_LIBTOM 1
#define DISABLE_LASTLOG 1
#define DISABLE_UTMP 1
#define DISABLE_UTMPX 1
#define DISABLE_WTMP 1
#define DISABLE_WTMPX 1
#define DISABLE_PAM 1
#define DISABLE_PUTUTLINE 1
#define DISABLE_PUTUTXLINE 1
#define DISABLE_SYSLOG 1

#define HAVE_BASENAME 1
#define HAVE_CLOCK_GETTIME 1
#define HAVE_CONST_GAI_STRERROR_PROTO 1
#define HAVE_FORK 1
#define HAVE_FREEADDRINFO 1
#define HAVE_GAI_STRERROR 1
#define HAVE_GETADDRINFO 1
#define HAVE_GETNAMEINFO 1
#define HAVE_GETPASS 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIBGEN_H 1
#define HAVE_LIBZ 1
#define HAVE_MACH_ABSOLUTE_TIME 1
#define HAVE_MACH_MACH_TIME_H 1
#define HAVE_NETDB_H 1
#define HAVE_NETINET_IN_H 1
#define HAVE_NETINET_IN_SYSTM_H 1
#define HAVE_NETINET_TCP_H 1
#define HAVE_PATHS_H 1
#define HAVE_PUTENV 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRINGS_H 1
#define HAVE_STRING_H 1
#define HAVE_STRLCAT 1
#define HAVE_STRLCPY 1
#define HAVE_STRUCT_ADDRINFO 1
#define HAVE_STRUCT_IN6_ADDR 1
#define HAVE_STRUCT_SOCKADDR_IN6 1
#define HAVE_STRUCT_SOCKADDR_STORAGE 1
#define HAVE_STRUCT_SOCKADDR_STORAGE_SS_FAMILY 1
#define HAVE_SYS_RANDOM_H 1
#define HAVE_SYS_SELECT_H 1
#define HAVE_SYS_SOCKET_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SYS_UIO_H 1
#define HAVE_SYS_WAIT_H 1
#define HAVE_UINT16_T 1
#define HAVE_UINT32_T 1
#define HAVE_UINT8_T 1
#define HAVE_U_INT16_T 1
#define HAVE_U_INT32_T 1
#define HAVE_U_INT8_T 1
#define HAVE_UNISTD_H 1
#define HAVE_WRITEV 1
#define SELECT_TYPE_ARG1 int
#define SELECT_TYPE_ARG234 (fd_set *)
#define SELECT_TYPE_ARG5 (struct timeval *)
#define STDC_HEADERS 1

#endif
