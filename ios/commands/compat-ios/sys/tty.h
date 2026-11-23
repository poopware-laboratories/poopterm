/*
 * <sys/tty.h>, missing from the iOS SDK: tabs includes it for the
 * terminal interfaces <termios.h> and <sys/ioctl.h> declare.
 */
#ifndef POOPTERM_SYS_TTY_H
#define POOPTERM_SYS_TTY_H
#include <sys/ioctl.h>
#include <termios.h>
#endif
