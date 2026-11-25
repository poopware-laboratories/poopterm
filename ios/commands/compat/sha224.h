/* libmd's SHA-224 (see poopterm_md.h). */
#ifndef POOPTERM_SHA224_H
#define POOPTERM_SHA224_H
#include "poopterm_md.h"
POOPTERM_MD(SHA224_, SHA224_CTX, CC_SHA256_CTX, CC_SHA224, CC_SHA224_DIGEST_LENGTH)
#endif
