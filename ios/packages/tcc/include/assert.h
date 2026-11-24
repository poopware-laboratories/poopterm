#ifndef _ASSERT_H
#define _ASSERT_H
extern void __assert_rtn(const char *, const char *, int, const char *) __attribute__((noreturn));
#ifdef NDEBUG
#define assert(e) ((void)0)
#else
#define assert(e) ((e) ? (void)0 : __assert_rtn(__func__, __FILE__, __LINE__, #e))
#endif
#endif
