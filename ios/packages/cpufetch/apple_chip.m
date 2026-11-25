/*
 * The name of the Apple chip cpufetch runs on, for chips its tables do
 * not know (every iPhone and iPad, and Macs newer than the M3). A chip
 * gives its name to its GPU, which Metal reports ("Apple A18 Pro GPU"):
 * the chip is that name without the "GPU". Without Metal, the Mac's brand
 * string, then the model identifier ("iPhone17,1").
 */
#include <stdlib.h>
#include <string.h>
#include <sys/sysctl.h>
#import <Metal/Metal.h>

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

char *poop_apple_chip_name(void) {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        const char *gpu = device ? device.name.UTF8String : NULL;
        if (gpu && *gpu) {
            char *name = strdup(gpu);
            size_t length = strlen(name);
            if (length > 4 && strcmp(name + length - 4, " GPU") == 0) {
                name[length - 4] = '\0';
            }
            return name;
        }
    }
    char *name = sysctl_string("machdep.cpu.brand_string");
    if (!name) {
        name = sysctl_string("hw.machine");
    }
    return name ? name : strdup("Apple silicon");
}
