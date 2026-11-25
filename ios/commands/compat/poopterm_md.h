/*
 * FreeBSD's libmd interface (MD5Init, SHA256_End…), which md5 and install
 * use, over the system's CommonCrypto digests. Each digest gets Init,
 * Update, Final, End (the digest in hex, in `buf` or a new allocation),
 * Data, Fd, FdChunk, File and FileChunk.
 */
#ifndef POOPTERM_MD_H
#define POOPTERM_MD_H

#include <CommonCrypto/CommonDigest.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/types.h>
#include <unistd.h>

static inline char *poopterm_md_hex(const unsigned char *digest, size_t len, char *buf) {
	static const char hex[] = "0123456789abcdef";
	if (buf == NULL && (buf = malloc(len * 2 + 1)) == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < len; i++) {
		buf[i * 2] = hex[digest[i] >> 4];
		buf[i * 2 + 1] = hex[digest[i] & 15];
	}
	buf[len * 2] = '\0';
	return buf;
}

/* NAME: libmd's prefix; CTX: the context type; CC: CommonCrypto's
 * function prefix; LEN: the digest's length in bytes. */
#define POOPTERM_MD(NAME, CTX, CCCTX, CC, LEN) \
	typedef CCCTX CTX; \
	static inline void NAME##Init(CTX *c) { CC##_Init(c); } \
	static inline void NAME##Update(CTX *c, const void *data, size_t len) { \
		const unsigned char *at = data; \
		while (len > 0) { \
			CC_LONG part = len > 0x40000000 ? 0x40000000 : (CC_LONG)len; \
			CC##_Update(c, at, part); \
			at += part; \
			len -= part; \
		} \
	} \
	static inline void NAME##Final(unsigned char digest[LEN], CTX *c) { CC##_Final(digest, c); } \
	static inline char *NAME##End(CTX *c, char *buf) { \
		unsigned char digest[LEN]; \
		CC##_Final(digest, c); \
		return poopterm_md_hex(digest, LEN, buf); \
	} \
	static inline char *NAME##Data(const void *data, unsigned int len, char *buf) { \
		CTX c; \
		NAME##Init(&c); \
		NAME##Update(&c, data, len); \
		return NAME##End(&c, buf); \
	} \
	static inline char *NAME##FdChunk(int fd, char *buf, off_t offset, off_t length) { \
		unsigned char block[16384]; \
		CTX c; \
		NAME##Init(&c); \
		if (offset != 0 && lseek(fd, offset, SEEK_SET) < 0) { \
			return NULL; \
		} \
		for (;;) { \
			size_t want = sizeof block; \
			if (length > 0 && (off_t)want > length) { \
				want = (size_t)length; \
			} \
			ssize_t got = want ? read(fd, block, want) : 0; \
			if (got < 0) { \
				return NULL; \
			} \
			if (got == 0) { \
				break; \
			} \
			NAME##Update(&c, block, (size_t)got); \
			if (length > 0 && (length -= got) == 0) { \
				break; \
			} \
		} \
		return NAME##End(&c, buf); \
	} \
	static inline char *NAME##Fd(int fd, char *buf) { return NAME##FdChunk(fd, buf, 0, 0); } \
	static inline char *NAME##FileChunk(const char *name, char *buf, off_t offset, off_t length) { \
		int fd = open(name, O_RDONLY | O_CLOEXEC); \
		if (fd < 0) { \
			return NULL; \
		} \
		char *made = NAME##FdChunk(fd, buf, offset, length); \
		int saved = errno; \
		close(fd); \
		errno = saved; \
		return made; \
	} \
	static inline char *NAME##File(const char *name, char *buf) { return NAME##FileChunk(name, buf, 0, 0); }

#endif
