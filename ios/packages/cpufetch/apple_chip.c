/*
 * The name of the Apple chip cpufetch runs on, for chips its tables do
 * not know (every iPhone and iPad, and Macs newer than the M3). A chip
 * gives its name to its GPU, which Metal reports ("Apple A18 Pro GPU"):
 * the chip is that name without the "GPU". Without Metal, the Mac's brand
 * string, then the model identifier ("iPhone17,1").
 *
 * Plain C over the Objective-C runtime's functions, not Objective-C: a
 * downloaded package is loaded by Poopterm's own loader, which binds
 * functions but does not register a library's Objective-C metadata.
 */
#include <stdlib.h>
#include <string.h>
#include <sys/sysctl.h>
#include <objc/message.h>
#include <objc/runtime.h>

extern id MTLCreateSystemDefaultDevice(void);
extern void *objc_autoreleasePoolPush(void);
extern void objc_autoreleasePoolPop(void *pool);

static char *sysctl_string(const char *name) {
    size_t size = 0;
    if (sysctlbyname(name, NULL, &size, NULL, 0) != 0 || size == 0) {
        return NULL;
    }
    char *value = calloc(1, size + 1);
    if (value && sysctlbyname(name, value, &size, NULL, 0) != 0) {
        free(value);
        return NULL;
    }
    return value;
}

/* The GPU's name, without " GPU", or NULL. */
static char *gpu_chip(void) {
    id (*send)(id, SEL) = (id (*)(id, SEL)) objc_msgSend;
    const char *(*send_string)(id, SEL) = (const char *(*)(id, SEL)) objc_msgSend;
    void *pool = objc_autoreleasePoolPush();
    id device = MTLCreateSystemDefaultDevice();
    char *name = NULL;
    if (device) {
        id text = send(device, sel_registerName("name"));
        const char *gpu = text ? send_string(text, sel_registerName("UTF8String")) : NULL;
        if (gpu && *gpu) {
            name = strdup(gpu);
            size_t length = strlen(name);
            if (length > 4 && strcmp(name + length - 4, " GPU") == 0) {
                name[length - 4] = '\0';
            }
        }
        send(device, sel_registerName("release"));
    }
    objc_autoreleasePoolPop(pool);
    return name;
}

char *poop_apple_chip_name(void) {
    char *name = gpu_chip();
    if (!name) {
        name = sysctl_string("machdep.cpu.brand_string");
    }
    if (!name) {
        name = sysctl_string("hw.machine");
    }
    return name ? name : strdup("Apple silicon");
}
