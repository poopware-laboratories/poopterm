/* libmd's SHA-256 (see poopterm_md.h). */
#ifndef POOPTERM_SHA256_H
#define POOPTERM_SHA256_H
#include "poopterm_md.h"
POOPTERM_MD(SHA256_, SHA256_CTX, CC_SHA256_CTX, CC_SHA256, CC_SHA256_DIGEST_LENGTH)
#endif
