/*
 * macOS's <get_compat.h>, missing from the iOS SDK: commands built for
 * Poopterm always take the conforming (UNIX 2003) behavior.
 */
#ifndef POOPTERM_GET_COMPAT_H
#define POOPTERM_GET_COMPAT_H
#define COMPAT_MODE(function, mode) 1
#endif
