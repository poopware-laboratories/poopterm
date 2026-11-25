/*
 * The GPU on iOS, as Metal names it ("Apple A17 Pro GPU"): IOKit, which
 * fastfetch reads on macOS, is not open to apps.
 */
#include "detection/gpu/gpu.h"
#import <Metal/Metal.h>

const char* ffDetectGPUImpl(const FFGPUOptions* options, FFlist* gpus) {
    (void) options;
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    if (!device) {
        return "Metal has no device";
    }
    FFGPUResult* gpu = FF_LIST_ADD(FFGPUResult, *gpus);
    gpu->index = FF_GPU_INDEX_UNSET;
    gpu->type = FF_GPU_TYPE_INTEGRATED;
    ffStrbufInitS(&gpu->vendor, FF_GPU_VENDOR_NAME_APPLE);
    ffStrbufInitS(&gpu->name, device.name.UTF8String);
    ffStrbufInit(&gpu->driver);
    ffStrbufInitStatic(&gpu->platformApi, "Metal");
    ffStrbufInit(&gpu->memoryType);
    gpu->temperature = FF_GPU_TEMP_UNSET;
    gpu->coreUsage = FF_GPU_CORE_USAGE_UNSET;
    gpu->coreCount = FF_GPU_CORE_COUNT_UNSET;
    gpu->frequency = FF_GPU_FREQUENCY_UNSET;
    gpu->pcieSpeed = FF_GPU_PCIE_SPEED_UNSET;
    gpu->dedicated.total = gpu->dedicated.used = FF_GPU_VMEM_SIZE_UNSET;
    gpu->shared.total = device.recommendedMaxWorkingSetSize;
    gpu->shared.used = device.currentAllocatedSize;
    gpu->deviceId = device.registryID;
    return NULL;
}
