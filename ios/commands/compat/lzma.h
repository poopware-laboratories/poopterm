/*
 * Declarations from liblzma, which the iOS and macOS SDKs ship as a
 * library but without its header: the stream decoder grep reads .xz and
 * .lzma files with, as liblzma's documentation gives them.
 */
#ifndef POOPTERM_LZMA_H
#define POOPTERM_LZMA_H
#include <stddef.h>
#include <stdint.h>

typedef enum {
	LZMA_OK = 0,
	LZMA_STREAM_END = 1,
	LZMA_NO_CHECK = 2,
	LZMA_UNSUPPORTED_CHECK = 3,
	LZMA_GET_CHECK = 4,
	LZMA_MEM_ERROR = 5,
	LZMA_MEMLIMIT_ERROR = 6,
	LZMA_FORMAT_ERROR = 7,
	LZMA_OPTIONS_ERROR = 8,
	LZMA_DATA_ERROR = 9,
	LZMA_BUF_ERROR = 10,
	LZMA_PROG_ERROR = 11,
} lzma_ret;

typedef enum {
	LZMA_RUN = 0,
	LZMA_SYNC_FLUSH = 1,
	LZMA_FULL_FLUSH = 2,
	LZMA_FINISH = 3,
} lzma_action;

/* Decoder flags. */
#define LZMA_TELL_NO_CHECK 0x01
#define LZMA_TELL_UNSUPPORTED_CHECK 0x02
#define LZMA_TELL_ANY_CHECK 0x04
#define LZMA_CONCATENATED 0x08

/* The stream: the caller's buffers, then fields only the library uses. */
typedef struct {
	const uint8_t *next_in;
	size_t avail_in;
	uint64_t total_in;
	uint8_t *next_out;
	size_t avail_out;
	uint64_t total_out;
	const void *allocator;
	void *internal;
	void *reserved_ptr1, *reserved_ptr2, *reserved_ptr3, *reserved_ptr4;
	uint64_t reserved_int1, reserved_int2;
	size_t reserved_int3, reserved_int4;
	int reserved_enum1, reserved_enum2;
} lzma_stream;

#define LZMA_STREAM_INIT {0}

lzma_ret lzma_stream_decoder(lzma_stream *strm, uint64_t memlimit, uint32_t flags);
lzma_ret lzma_alone_decoder(lzma_stream *strm, uint64_t memlimit);
lzma_ret lzma_code(lzma_stream *strm, lzma_action action);
void lzma_end(lzma_stream *strm);
#endif
