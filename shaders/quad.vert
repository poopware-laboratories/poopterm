#version 450
// Places each quad: one instance a quad, six vertices (two triangles)
// covering what of it lies inside its clip. Italic coverage is sheared
// right by up to a sixth of its height, as the software renderer does, so
// the triangles reach that far.

struct Quad {
	// Left, top, width and height, in pixels.
	ivec4 rect;
	// Left, top, right and bottom.
	ivec4 clip;
	// Atlas x | y << 16, page, kind | italic << 8, color 0xAARRGGBB.
	uvec4 source;
};

layout(std430, set = 0, binding = 0) readonly buffer Quads {
	Quad quads[];
};

layout(push_constant) uniform Target {
	// Width and height, in pixels.
	vec2 size;
} target;

const uint COVERAGE = 1u;

layout(location = 0) flat out uint index;

void main() {
	Quad quad = quads[gl_InstanceIndex];
	bool italic = (quad.source.z >> 8 & 1u) != 0u && (quad.source.z & 0xffu) == COVERAGE;
	int shear = italic ? quad.rect.w / 6 : 0;
	ivec2 start = max(quad.rect.xy, quad.clip.xy);
	ivec2 end = min(quad.rect.xy + quad.rect.zw + ivec2(shear, 0), quad.clip.zw);
	end = max(end, start);
	// Corners 0 1 2, 2 1 3 of the rectangle start to end.
	uint corner = uint[](0u, 1u, 2u, 2u, 1u, 3u)[gl_VertexIndex];
	vec2 at = vec2((corner & 1u) != 0u ? end.x : start.x, (corner & 2u) != 0u ? end.y : start.y);
	gl_Position = vec4(at / target.size * 2.0 - 1.0, 0.0, 1.0);
	index = gl_InstanceIndex;
}
