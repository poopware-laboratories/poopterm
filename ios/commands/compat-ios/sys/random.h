/*
 * <sys/random.h>, missing from the iOS SDK: `getentropy` (sort's random
 * seed) from `arc4random_buf`, with getentropy's limit of 256 bytes.
 */
#ifndef POOPTERM_SYS_RANDOM_H
#define POOPTERM_SYS_RANDOM_H
#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
static inline int getentropy(void *buffer, size_t size) {
	if (size > 256) {
		errno = EIO;
		return -1;
	}
	arc4random_buf(buffer, size);
	return 0;
}
#endif
