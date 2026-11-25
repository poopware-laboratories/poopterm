#ifndef _TIME_H
#define _TIME_H
#include <_poopterm.h>
#define CLOCKS_PER_SEC 1000000
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 6
typedef int clockid_t;
struct timespec { long tv_sec; long tv_nsec; };
struct tm {
	int tm_sec, tm_min, tm_hour, tm_mday, tm_mon, tm_year, tm_wday, tm_yday, tm_isdst;
	long tm_gmtoff;
	char *tm_zone;
};
time_t time(time_t *);
clock_t clock(void);
double difftime(time_t, time_t);
time_t mktime(struct tm *);
struct tm *gmtime(const time_t *);
struct tm *localtime(const time_t *);
struct tm *gmtime_r(const time_t *, struct tm *);
struct tm *localtime_r(const time_t *, struct tm *);
size_t strftime(char *, size_t, const char *, const struct tm *);
char *ctime(const time_t *);
char *asctime(const struct tm *);
int nanosleep(const struct timespec *, struct timespec *);
int clock_gettime(clockid_t, struct timespec *);
#endif
