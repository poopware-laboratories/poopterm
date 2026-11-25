/*
 * The CPU on iOS. fastfetch asks for machdep.cpu.brand_string, which iOS
 * does not have, so the CPU line was left out. A phone's chip gives its
 * name to its GPU, which Metal reports ("Apple A18 GPU"): the chip is
 * that name without the "GPU". Core counts come from sysctl, which an
 * app may read.
 */
#include "detection/cpu/cpu.h"
#include "common/sysctl.h"
#include "common/strutil.h"
#import <Metal/Metal.h>

/* "Apple A18 GPU" -> "Apple A18"; any other name is left as it is. */
static void chipName(const char* gpuName, FFstrbuf* result) {
    ffStrbufSetS(result, gpuName);
    if (ffStrbufEndsWithS(result, " GPU")) {
        ffStrbufSubstrBefore(result, result->length - strlen(" GPU"));
    }
}

const char* ffDetectCPUImpl(const FFCPUOptions* options, FFCPUResult* cpu) {
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    if (device) {
        chipName(device.name.UTF8String, &cpu->name);
    }
    if (cpu->name.length == 0) {
        /* No Metal: the model identifier ("iPhone17,5") is better than nothing. */
        ffSysctlGetString("hw.machine", &cpu->name);
    }
    if (cpu->name.length == 0) {
        return "no name for the chip";
    }
    ffStrbufSetStatic(&cpu->vendor, "Apple");
    cpu->packages = 1;

    cpu->coresPhysical = (uint16_t) ffSysctlGetInt("hw.physicalcpu_max", 1);
    if (cpu->coresPhysical == 1) {
        cpu->coresPhysical = (uint16_t) ffSysctlGetInt("hw.physicalcpu", 1);
    }
    cpu->coresLogical = (uint16_t) ffSysctlGetInt("hw.logicalcpu_max", 1);
    if (cpu->coresLogical == 1) {
        cpu->coresLogical = (uint16_t) ffSysctlGetInt("hw.ncpu", 1);
    }
    cpu->coresOnline = (uint16_t) ffSysctlGetInt("hw.logicalcpu", 1);
    if (cpu->coresOnline == 1) {
        cpu->coresOnline = (uint16_t) ffSysctlGetInt("hw.activecpu", 1);
    }

    /* Performance and efficiency cores, where the system says. */
    uint32_t levels = (uint32_t) ffSysctlGetInt("hw.nperflevels", 0);
    if (options->showPeCoreCount && levels > 0) {
        if (levels > ARRAY_SIZE(cpu->coreTypes)) {
            levels = ARRAY_SIZE(cpu->coreTypes);
        }
        char key[] = "hw.perflevelN.logicalcpu";
        for (uint32_t i = 0; i < levels; ++i) {
            key[strlen("hw.perflevel")] = (char) ('0' + i);
            cpu->coreTypes[i] = (FFCPUCore) {
                .freq = levels - i,
                .count = (uint32_t) ffSysctlGetInt(key, 0),
            };
        }
    }
    cpu->temperature = FF_CPU_TEMP_UNSET;
    return NULL;
}
