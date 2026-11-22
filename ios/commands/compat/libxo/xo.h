/*
 * Declarations from libxo, which the iOS SDK ships as a library but
 * without its header (wc and df emit through it). Its `xo_err` calls
 * would end the app, so they end the command instead.
 */
#ifndef POOPTERM_XO_H
#define POOPTERM_XO_H
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>

typedef struct xo_handle_s xo_handle_t;
typedef unsigned xo_style_t;
typedef unsigned long long xo_xof_flags_t;

#define XO_STYLE_TEXT 0
#define XO_STYLE_XML 1
#define XO_STYLE_JSON 2
#define XO_STYLE_HTML 3

int xo_parse_args(int argc, char **argv);
void xo_set_version(const char *version);
xo_handle_t *xo_create_to_file(FILE *fp, xo_style_t style, xo_xof_flags_t flags);
xo_style_t xo_get_style(xo_handle_t *xop);
int xo_emit(const char *fmt, ...);
int xo_emit_h(xo_handle_t *xop, const char *fmt, ...);
int xo_attr(const char *name, const char *fmt, ...);
int xo_open_container(const char *name);
int xo_close_container(const char *name);
int xo_open_list(const char *name);
int xo_close_list(const char *name);
int xo_open_instance(const char *name);
int xo_close_instance(const char *name);
int xo_finish(void);
void xo_warn(const char *fmt, ...);
void xo_warnx(const char *fmt, ...);
void xo_error(const char *fmt, ...);

void poopterm_exit(int status) __attribute__((noreturn));
#define xo_err(status, ...) (xo_warn(__VA_ARGS__), poopterm_exit(status))
#define xo_errx(status, ...) (xo_warnx(__VA_ARGS__), poopterm_exit(status))
#endif
