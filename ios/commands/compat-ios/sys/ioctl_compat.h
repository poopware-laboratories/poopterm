/*
 * <sys/ioctl_compat.h>, missing from the iOS SDK: the line discipline
 * numbers stty names (the device only ever has the standard one).
 */
#ifndef POOPTERM_IOCTL_COMPAT_H
#define POOPTERM_IOCTL_COMPAT_H
#include <sys/ioctl.h>
#define OTTYDISC 0
#define NETLDISC 1
#define NTTYDISC 2
#endif
