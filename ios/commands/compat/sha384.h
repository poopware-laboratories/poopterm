/* libmd's SHA-384 (see poopterm_md.h). */
#ifndef POOPTERM_SHA384_H
#define POOPTERM_SHA384_H
#include "poopterm_md.h"
POOPTERM_MD(SHA384_, SHA384_CTX, CC_SHA512_CTX, CC_SHA384, CC_SHA384_DIGEST_LENGTH)
#endif
