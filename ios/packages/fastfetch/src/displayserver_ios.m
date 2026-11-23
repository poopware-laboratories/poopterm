/*
 * The screen on iOS, from UIScreen: its pixels, refresh rate and scale.
 * There is no window manager to name beyond the system's own.
 */
#include "detection/displayserver/displayserver.h"
#import <UIKit/UIKit.h>

void ffConnectDisplayServerImpl(FFDisplayServerResult* ds) {
    ffStrbufSetStatic(&ds->wmProcessName, "SpringBoard");
    ffStrbufSetStatic(&ds->wmPrettyName, "SpringBoard");
    UIScreen* screen = UIScreen.mainScreen;
    if (!screen) {
        return;
    }
    CGRect pixels = screen.nativeBounds;
    uint32_t width = (uint32_t) pixels.size.width;
    uint32_t height = (uint32_t) pixels.size.height;
    double rate = (double) screen.maximumFramesPerSecond;
    FF_STRBUF_AUTO_DESTROY name = ffStrbufCreateStatic("Built-in Display");
    ffdsAppendDisplay(ds,
        width,
        height,
        rate,
        (uint32_t) (96 * screen.nativeScale),
        width,
        height,
        rate,
        0,
        &name,
        FF_DISPLAY_TYPE_BUILTIN,
        true,
        0,
        0,
        0,
        "UIKit");
}
