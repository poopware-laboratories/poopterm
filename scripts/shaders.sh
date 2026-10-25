#!/bin/sh
# Compiles the GLSL in shaders/ to the SPIR-V checked in beside it, with
# the glslc of the Vulkan SDK pinned below (shaderc 2026.4, which distros
# package too), and validates it. With
# --check, compiles to a temporary directory instead and fails unless the
# checked-in SPIR-V is what the source compiles to.
set -eu

SDK=1.4.363.0
SHADERC=2026.4
cd "$(dirname "$0")/.."
GLSLC=${GLSLC:-glslc}
SPIRV_VAL=${SPIRV_VAL:-spirv-val}

if ! "$GLSLC" --version | grep -q "$SHADERC"; then
	echo "glslc is not shaderc $SHADERC, of Vulkan SDK $SDK, which the checked-in SPIR-V comes from" >&2
	exit 1
fi

out=shaders
if [ "${1:-}" = --check ]; then
	out=$(mktemp -d)
	trap 'rm -rf "$out"' EXIT
fi

status=0
for source in shaders/*.vert shaders/*.frag; do
	name=$(basename "$source")
	"$GLSLC" --target-env=vulkan1.3 -O -Werror -o "$out/$name.spv" "$source"
	"$SPIRV_VAL" --target-env vulkan1.3 "$out/$name.spv"
	if [ "$out" != shaders ] && ! cmp -s "$out/$name.spv" "shaders/$name.spv"; then
		echo "shaders/$name.spv is not what $source compiles to; run scripts/shaders.sh" >&2
		status=1
	fi
done
exit $status
