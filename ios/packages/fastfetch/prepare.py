#!/usr/bin/env python3
"""Adds an iOS branch to fastfetch's CMakeLists.txt, before its macOS one.

iOS shares macOS's sources where its SDK has what they use. Where it does
not (IOKit, AppKit, CoreWLAN, IOBluetooth, CoreGraphics displays...) a
module takes fastfetch's own *_nosupport.c, a portable version (DNS from
resolv.conf), or a file of ours in src/ (the GPU through Metal, the screen
through UIKit, and what an app cannot reach at all). Run on a checkout;
running it again changes nothing.

    prepare.py <fastfetch checkout>
"""
import sys
from pathlib import Path

MARK = "# Poopterm: iOS"

# macOS source -> what iOS builds instead (None: nothing).
REPLACE = {
    "src/common/apple/osascript.m": None,
    "src/common/impl/netif_apple.c": None,
    "src/common/impl/processing_linux.c": None,
    "src/common/impl/kmod_apple.c": "src/common/impl/kmod_nosupport.c",
    "src/detection/dns/dns_apple.c": "src/detection/dns/dns_linux.c",
    "src/detection/displayserver/displayserver_apple.c": None,
    "src/detection/font/font_apple.m": None,
    "src/detection/gpu/gpu_apple.c": None,
    "src/detection/gpu/gpu_apple.m": None,
    "src/detection/localip/localip_apple.c": None,
    "src/detection/opengl/opengl_apple.c": None,
    "src/detection/processes/processes_apple.c": None,
    "src/detection/sound/sound_apple.c": None,
    "src/detection/top/top_apple.c": None,
    "src/detection/wallpaper/wallpaper_apple.m": None,
    "src/detection/media/media_apple.m": None,
    "src/detection/codec/codec_apple.c": None,
}
NOSUPPORT = """battery bluetooth bluetoothradio brightness camera diskio gamepad
keyboard mouse netio physicaldisk poweradapter wifi wm bios board chassis
physicalmemory""".split()
OURS = ["unsupported_ios.c", "gpu_ios.m", "displayserver_ios.m"]
FRAMEWORKS = ["CoreFoundation", "Foundation", "IOKit", "Metal", "UIKit"]


def ios_sources(apple_block: str) -> list[str]:
    sources = []
    for line in apple_block.splitlines():
        source = line.strip()
        # The x86-64 branch (Windows-style firmware tables, the shared
        # physical memory reader) and the image
        # logo are left out.
        if not source.startswith("src/") or "_windows" in source or "imageio" in source or "_shared" in source:
            continue
        module = source.split("/")[2] if source.startswith("src/detection/") else ""
        if source in REPLACE:
            source = REPLACE[source]
        elif module in NOSUPPORT and source.endswith(("_apple.c", "_apple.m")):
            source = f"src/detection/{module}/{module}_nosupport.c"
        if source and source not in sources:
            sources.append(source)
    return sources


def main() -> None:
    path = Path(sys.argv[1]) / "CMakeLists.txt"
    text = path.read_text()
    if MARK in text:
        return
    start = text.index("elseif(APPLE)\n    list(APPEND LIBFASTFETCH_SRC")
    end = text.index("elseif(WIN32)", start)
    sources = ios_sources(text[start:end])
    branch = "\n".join(f"        {source}" for source in sources)
    ours = "\n".join(f"        ${{POOPTERM_FASTFETCH_IOS}}/src/{name}" for name in OURS)
    block = f"""{MARK}
elseif(IOS)
    list(APPEND LIBFASTFETCH_SRC
{branch}
{ours}
    )
    list(REMOVE_ITEM LIBFASTFETCH_SRC
        src/detection/opencl/opencl.c
        src/detection/opengl/opengl_shared.c
    )
"""
    text = text[:start] + block + text[start:]
    link = text.index('elseif(APPLE)\n    target_link_libraries(libfastfetch\n        PRIVATE "-framework AVFoundation"')
    frameworks = "\n".join(f'        PRIVATE "-framework {name}"' for name in FRAMEWORKS)
    text = text[:link] + f"""{MARK}
elseif(IOS)
    target_link_libraries(libfastfetch
{frameworks}
    )
    # The command shim (exit ends the command, not the app), for fastfetch's
    # own code only: CMake's compiler checks need a real main.
    target_compile_options(libfastfetch PRIVATE ${{POOPTERM_FASTFETCH_FLAGS}})
""" + text[link:]
    # fastfetch.c holds main, so it takes the shim too, keeping main's name
    # for C's implicit return 0 (build.sh aliases it); its target is
    # defined last.
    text += f"""
{MARK}
if(IOS)
    target_compile_options(fastfetch PRIVATE ${{POOPTERM_FASTFETCH_FLAGS}} -DPOOPTERM_KEEP_MAIN)
endif()
"""
    path.write_text(text)


if __name__ == "__main__":
    main()
