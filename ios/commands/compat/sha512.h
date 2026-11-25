/* libmd's SHA-512 (see poopterm_md.h). */
#ifndef POOPTERM_SHA512_H
#define POOPTERM_SHA512_H
#include "poopterm_md.h"
POOPTERM_MD(SHA512_, SHA512_CTX, CC_SHA512_CTX, CC_SHA512, CC_SHA512_DIGEST_LENGTH)
#endif
