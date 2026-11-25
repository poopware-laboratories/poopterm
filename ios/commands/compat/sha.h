/* libmd's SHA-1 (see poopterm_md.h), and the SHA-2 family install uses. */
#ifndef POOPTERM_SHA_H
#define POOPTERM_SHA_H
#include "poopterm_md.h"
POOPTERM_MD(SHA1_, SHA1_CTX, CC_SHA1_CTX, CC_SHA1, CC_SHA1_DIGEST_LENGTH)
#include "sha256.h"
#include "sha512.h"
#endif
