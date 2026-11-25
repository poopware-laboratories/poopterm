/*
 * Declarations from libutil, which the iOS SDK ships as a library but
 * without its header: number formatting used by du, df, tail and ls.
 */
#ifndef POOPTERM_LIBUTIL_H
#define POOPTERM_LIBUTIL_H
#include <stddef.h>
#include <stdint.h>

#define HN_DECIMAL 0x01
#define HN_NOSPACE 0x02
#define HN_B 0x04
#define HN_DIVISOR_1000 0x08
#define HN_GETSCALE 0x10
#define HN_AUTOSCALE 0x20

int humanize_number(char *buf, size_t len, int64_t number, const char *suffix, int scale, int flags);
int expand_number(const char *buf, uint64_t *num);
#endif
