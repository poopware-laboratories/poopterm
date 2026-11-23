/*
 * What fastfetch asks of the system that an iOS app cannot do or reach:
 * no child processes (fork and exec are not allowed), no AppleScript,
 * routing table, OpenGL, OpenCL, sound devices, process lists, fonts,
 * wallpaper, now playing (a private framework) or the video codecs
 * macOS lists. Each answers as fastfetch's own *_nosupport.c files do, with
 * the reason, and the module is left out of the output.
 */
#include "fastfetch.h"
#include "common/netif.h"
#include "common/processing.h"
#include "detection/codec/codec.h"
#include "detection/font/font.h"
#include "detection/media/media.h"
#include "detection/opencl/opencl.h"
#include "detection/opengl/opengl.h"
#include "detection/processes/processes.h"
#include "detection/sound/sound.h"
#include "detection/top/top.h"
#include "detection/wallpaper/wallpaper.h"

#define FF_IOS_NO "Not supported on iOS"

bool ffOsascript(const char* input, FFstrbuf* result) {
    (void) input;
    (void) result;
    return false;
}

bool ffNetifGetDefaultRouteImplV4(FFNetifDefaultRouteResult* result) {
    (void) result;
    return false;
}

bool ffNetifGetDefaultRouteImplV6(FFNetifDefaultRouteResult* result) {
    (void) result;
    return false;
}

const char* ffProcessSpawn(char* const argv[], FFProcessOutputType useOutput, FFNativeFD stdinFd, FFProcessHandle* outHandle) {
    (void) argv;
    (void) useOutput;
    (void) stdinFd;
    outHandle->pid = -1;
    outHandle->pipeRead = -1;
    return "iOS apps cannot start other programs";
}

const char* ffProcessReadOutput(FFProcessHandle* handle, FFstrbuf* buffer) {
    (void) handle;
    (void) buffer;
    return "iOS apps cannot start other programs";
}

void ffProcessGetInfoLinux(pid_t pid, FFstrbuf* processName, FFstrbuf* exe, const char** exeName, FFstrbuf* exePath) {
    (void) pid;
    (void) processName;
    (void) exePath;
    *exeName = exe->chars;
}

const char* ffProcessGetBasicInfoLinux(pid_t pid, FFstrbuf* name, pid_t* ppid, int32_t* tty) {
    (void) pid;
    (void) name;
    if (ppid) *ppid = 0;
    if (tty) *tty = -1;
    return FF_IOS_NO;
}

const char* ffDetectFont(FFFontResult* font) {
    (void) font;
    return FF_IOS_NO;
}

const char* ffDetectSound(FFSoundOptions* options, FFlist* devices) {
    (void) options;
    (void) devices;
    return FF_IOS_NO;
}

const char* ffDetectTopProcesses(FFTopOptions* options, FFlist* result) {
    (void) options;
    (void) result;
    return FF_IOS_NO;
}

const char* ffTopGetProcessSnapshot(FFlist* snapshots, FFTopTypes showTypes) {
    (void) snapshots;
    (void) showTypes;
    return FF_IOS_NO;
}

const char* ffDetectWallpaper(FFstrbuf* result) {
    (void) result;
    return FF_IOS_NO;
}

const char* ffDetectProcesses(const FFProcessesOptions* options, FFProcessesResult* result) {
    (void) options;
    (void) result;
    return FF_IOS_NO;
}

const char* ffDetectOpenGL(FFOpenGLOptions* options, FFOpenGLResult* result) {
    (void) options;
    (void) result;
    return FF_IOS_NO;
}

FFOpenCLResult* ffDetectOpenCL(void) {
    static FFOpenCLResult result;
    static bool initialized;
    if (!initialized) {
        initialized = true;
        ffStrbufInit(&result.version);
        ffStrbufInit(&result.name);
        ffStrbufInit(&result.vendor);
        ffListInit(&result.gpus);
        result.error = FF_IOS_NO;
    }
    return &result;
}

void ffLocalIpFixRedactedMacs(FFlist* results) {
    (void) results;
}

void ffDetectMediaImpl(FFMediaResult* media, bool saveCover) {
    (void) saveCover;
    ffStrbufAppendS(&media->error, FF_IOS_NO);
}

const char* ffDetectCodecNative(FFCodecOptions* options, FFlist* result) {
    (void) options;
    (void) result;
    return FF_IOS_NO;
}
