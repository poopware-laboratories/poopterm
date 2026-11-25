#ifndef _SYS_TIME_H
#define _SYS_TIME_H
#include <time.h>
struct timeval { long tv_sec; int tv_usec; };
int gettimeofday(struct timeval *, void *);
#endif
