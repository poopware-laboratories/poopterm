#ifndef _SETJMP_H
#define _SETJMP_H
typedef int jmp_buf[48];
int setjmp(jmp_buf);
void longjmp(jmp_buf, int) __attribute__((noreturn));
#endif
