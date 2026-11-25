/*
 * macOS's private <membershipPriv.h>. ls uses it only to name the owner of
 * an ACL entry (`ls -le`); on iOS the lookup always fails, so ls prints the
 * entry's UUID instead, as it does for `ls -len`.
 */
#ifndef POOPTERM_MEMBERSHIPPRIV_H
#define POOPTERM_MEMBERSHIPPRIV_H
#include <membership.h>
#include <stddef.h>
#define ID_TYPE_NAME 100
#define MBR_REC_TYPE_USER 1
static inline int mbr_identifier_translate(int id_type, const void *identifier,
	size_t identifier_size, int target_type, void **result, int *rec_type) {
	(void)id_type, (void)identifier, (void)identifier_size, (void)target_type;
	(void)result, (void)rec_type;
	return -1;
}
#endif
