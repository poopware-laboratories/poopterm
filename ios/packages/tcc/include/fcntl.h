#ifndef _FCNTL_H
#define _FCNTL_H
#include <_poopterm.h>
#define O_RDONLY 0x0000
#define O_WRONLY 0x0001
#define O_RDWR 0x0002
#define O_NONBLOCK 0x0004
#define O_APPEND 0x0008
#define O_CREAT 0x0200
#define O_TRUNC 0x0400
#define O_EXCL 0x0800
#define F_GETFD 1
#define F_SETFD 2
#define F_GETFL 3
#define F_SETFL 4
#define FD_CLOEXEC 1
int open(const char *, int, ...);
int creat(const char *, mode_t);
int fcntl(int, int, ...);
#endif
