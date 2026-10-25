#version 450
// Fills each pixel of a quad: a solid color, a glyph's coverage in its
// color, or a color glyph's own texels, read texel for texel from the
// atlas, unfiltered. Blending with what is below is the pipeline's; a
// pixel left out is transparent, which leaves what is below as it was, so
// nothing needs discarding (which Vulkan 1.3 turns into a demotion, a
// feature the device does not turn on).

struct Quad {
	ivec4 rect;
	ivec4 clip;
	uvec4 source;
};

layout(std430, set = 0, binding = 0) readonly buffer Quads {
	Quad quads[];
};

layout(set = 0, binding = 1) uniform sampler2DArray atlas;

const uint SOLID = 0u;
const uint COVERAGE = 1u;
const uint COLOR = 2u;

layout(location = 0) flat in uint index;

layout(location = 0) out vec4 color;

void main() {
	Quad quad = quads[index];
	uint kind = quad.source.z & 0xffu;
	// 0xAARRGGBB unpacks to blue, green, red, alpha.
	vec3 ink = unpackUnorm4x8(quad.source.w).zyx;
	if (kind == SOLID) {
		color = vec4(ink, 1.0);
		return;
	}
	ivec2 pixel = ivec2(gl_FragCoord.xy) - quad.rect.xy;
	bool italic = (quad.source.z >> 8 & 1u) != 0u && kind == COVERAGE;
	pixel.x -= italic ? (quad.rect.w - pixel.y) / 6 : 0;
	if (kind > COLOR || pixel.x < 0 || pixel.x >= quad.rect.z) {
		color = vec4(0.0);
		return;
	}
	ivec2 slot = ivec2(quad.source.x & 0xffffu, quad.source.x >> 16);
	vec4 texel = texelFetch(atlas, ivec3(slot + pixel, int(quad.source.y)), 0);
	color = kind == COVERAGE ? vec4(ink, texel.a) : texel;
}
