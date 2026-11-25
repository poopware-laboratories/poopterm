#ifndef _DIRENT_H
#define _DIRENT_H
typedef struct _telldir DIR;
struct dirent {
	unsigned long long d_ino, d_seekoff;
	unsigned short d_reclen, d_namlen;
	unsigned char d_type;
	char d_name[1024];
};
#define DT_UNKNOWN 0
#define DT_DIR 4
#define DT_REG 8
#define DT_LNK 10
DIR *opendir(const char *);
struct dirent *readdir(DIR *);
int closedir(DIR *);
void rewinddir(DIR *);
#endif
