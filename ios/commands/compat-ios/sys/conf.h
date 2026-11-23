/* What hexdump needs of <sys/conf.h>, which the iOS SDK leaves out: the
 * device types FIODTYPE reports. */
#ifndef POOPTERM_SYS_CONF_H
#define POOPTERM_SYS_CONF_H
#define D_TAPE 1
#define D_DISK 2
#define D_TTY 3
#endif
