/*
 * Included after poopterm_cmd.h when building for iOS: what the iOS SDK
 * marks unavailable, answered as a sandboxed app would be refused.
 */
#ifndef POOPTERM_IOS_H
#define POOPTERM_IOS_H
#include <errno.h>
#include <time.h>
/* An app may not set the clock (`date 0101…`). */
#define clock_settime(clock, time) (errno = EPERM, -1)
#endif
