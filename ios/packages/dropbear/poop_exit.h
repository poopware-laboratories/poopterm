/* Included before every source of the ssh server: `exit` ends the
 * connection being served, not the app. */
#ifndef POOPTERM_EXIT_H
#define POOPTERM_EXIT_H
#include <stdlib.h>
void poop_exit(int code) __attribute__((noreturn));
#define exit(code) poop_exit(code)
#endif
