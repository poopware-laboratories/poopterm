// The quad shaders in the Metal Shading Language, as quad.vert and
// quad.frag are for Vulkan, compiled by Metal when the renderer starts.
//
// The vertex shader places each quad: one instance a quad, six vertices
// (two triangles) covering what of it lies inside its clip. Italic coverage
// is sheared right by up to a sixth of its height, as the software renderer
// does, so the triangles reach that far.
//
// The fragment shader fills each pixel: a solid color, a glyph's coverage
// in its color, or a color glyph's own texels, read texel for texel from
// the atlas, unfiltered; or an image's texels, the part of it in its source
// rectangle scaled to the quad's size nearest-neighbor, as software does.
// Blending with what is below is the pipeline's; a pixel left out is
// transparent, which leaves what is below as it was.
#include <metal_stdlib>
using namespace metal;

struct Quad {
	// Left, top, width and height, in pixels.
	int4 rect;
	// Left, top, right and bottom.
	int4 clip;
	// Atlas x | y << 16, page, kind | italic << 8, color 0xAARRGGBB.
	uint4 source;
};

constant uint SOLID = 0u;
constant uint COVERAGE = 1u;
constant uint COLOR = 2u;
constant uint IMAGE = 3u;

struct Placed {
	float4 position [[position]];
	uint index [[flat]];
};

vertex Placed quad_vertex(
	uint vertex_id [[vertex_id]],
	// Counted from the draw's base instance, so each run reads its own.
	uint instance [[instance_id]],
	const device Quad *quads [[buffer(0)]],
	// Width and height, in pixels.
	constant float2 &size [[buffer(1)]]
) {
	Quad quad = quads[instance];
	bool italic = (quad.source.z >> 8 & 1u) != 0u && (quad.source.z & 0xffu) == COVERAGE;
	int shear = italic ? quad.rect.w / 6 : 0;
	int2 start = max(quad.rect.xy, quad.clip.xy);
	int2 end = min(quad.rect.xy + quad.rect.zw + int2(shear, 0), quad.clip.zw);
	end = max(end, start);
	// Corners 0 1 2, 2 1 3 of the rectangle start to end.
	const uint corners[6] = {0u, 1u, 2u, 2u, 1u, 3u};
	uint corner = corners[vertex_id];
	float2 at = float2((corner & 1u) != 0u ? end.x : start.x, (corner & 2u) != 0u ? end.y : start.y);
	float2 device_at = at / size * 2.0 - 1.0;
	Placed placed;
	// Metal's y points up, where pixel rows count down.
	placed.position = float4(device_at.x, -device_at.y, 0.0, 1.0);
	placed.index = instance;
	return placed;
}

fragment float4 quad_fragment(
	Placed placed [[stage_in]],
	const device Quad *quads [[buffer(0)]],
	texture2d_array<float, access::read> atlas [[texture(0)]],
	// The image the quads being drawn show, bound for each run of them.
	texture2d<float, access::read> image [[texture(1)]]
) {
	Quad quad = quads[placed.index];
	uint kind = quad.source.z & 0xffu;
	// 0xAARRGGBB unpacks to blue, green, red, alpha.
	float3 ink = unpack_unorm4x8_to_float(quad.source.w).zyx;
	if (kind == SOLID) {
		return float4(ink, 1.0);
	}
	int2 pixel = int2(placed.position.xy) - quad.rect.xy;
	if (kind == IMAGE) {
		// Source x | y << 16 and width | height << 16; integer division
		// picks the texel software picks.
		uint2 from = uint2(quad.source.x & 0xffffu, quad.source.x >> 16);
		uint2 size = uint2(quad.source.y & 0xffffu, quad.source.y >> 16);
		uint2 texel = from + uint2(pixel) * size / uint2(quad.rect.zw);
		return image.read(texel);
	}
	bool italic = (quad.source.z >> 8 & 1u) != 0u && kind == COVERAGE;
	pixel.x -= italic ? (quad.rect.w - pixel.y) / 6 : 0;
	if (kind > COLOR || pixel.x < 0 || pixel.x >= quad.rect.z) {
		return float4(0.0);
	}
	int2 slot = int2(quad.source.x & 0xffffu, quad.source.x >> 16);
	float4 texel = atlas.read(uint2(slot + pixel), quad.source.y);
	return kind == COVERAGE ? float4(ink, texel.a) : texel;
}
