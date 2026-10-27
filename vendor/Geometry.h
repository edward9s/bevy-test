#pragma once
#include <math/vec2.h>
#include <math/vec3.h>
#include <math/vec4.h>
#include <math/quat.h>
#include <cstdint>
#include <vector>

namespace forest {
using namespace filament::math;
struct Vertex {
    float3 position;
    quatf tangent;
    float2 uv;
    float4 color;
};
enum class Surface { Ground, Stone, Bark, Leaves, Canopy };
struct Mesh {
    Surface surface;
    std::vector<Vertex> vertices;
    std::vector<uint32_t> indices;
};
float elevation(float x, float z);
std::vector<Mesh> generate();
}
