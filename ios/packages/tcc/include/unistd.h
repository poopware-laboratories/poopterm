#ifndef _UNISTD_H
#define _UNISTD_H
#include <_poopterm.h>
#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2
#define F_OK 0
#define X_OK 1
#define W_OK 2
#define R_OK 4
#define _SC_ARG_MAX 1
#define _SC_CLK_TCK 3
#define _SC_PAGESIZE 29
#define _SC_NPROCESSORS_ONLN 58
ssize_t read(int, void *, size_t);
ssize_t write(int, const void *, size_t);
int close(int);
off_t lseek(int, off_t, int);
int dup(int);
int dup2(int, int);
int pipe(int[2]);
int isatty(int);
int fsync(int);
int ftruncate(int, off_t);
int unlink(const char *);
int rmdir(const char *);
int chdir(const char *);
char *getcwd(char *, size_t);
int access(const char *, int);
pid_t getpid(void);
pid_t getppid(void);
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
unsigned sleep(unsigned);
int usleep(unsigned);
long sysconf(int);
int getopt(int, char *const[], const char *);
extern char *optarg;
extern int optind, opterr, optopt;
#endif
