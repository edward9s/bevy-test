#include "../vendor/Geometry.h"
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <iostream>
#include <limits>
#include <stdexcept>

using filament::math::float2;
using filament::math::float3;
using namespace filament::math;

static void write_u32(std::ofstream& stream, uint32_t value) {
    stream.write(reinterpret_cast<const char*>(&value), sizeof(value));
}
static void write_f32(std::ofstream& stream, float value) {
    if (!std::isfinite(value)) throw std::runtime_error("Non-finite geometry data");
    stream.write(reinterpret_cast<const char*>(&value), sizeof(value));
}
static void write_vec3(std::ofstream& stream, float3 v) {
    write_f32(stream, v.x); write_f32(stream, v.y); write_f32(stream, v.z);
}
static void write_vec4(std::ofstream& stream, float x, float y, float z, float w) {
    write_f32(stream, x); write_f32(stream, y); write_f32(stream, z); write_f32(stream, w);
}
int main(int argc, char** argv) {
    try {
        if (argc != 2) throw std::runtime_error("Usage: export_geometry PATH");
        auto meshes = forest::generate();
        std::ofstream stream(argv[1], std::ios::binary);
        if (!stream) throw std::runtime_error("Cannot open output");
        stream.write("GEO1", 4);
        write_u32(stream, uint32_t(meshes.size()));
        uint64_t triangles = 0;
        for (const auto& mesh : meshes) {
            const auto& vertices = mesh.vertices;
            if (vertices.size() % 3 || mesh.indices.size() % 3)
                throw std::runtime_error("Unexpected nontriangular mesh");
            write_u32(stream, uint32_t(mesh.surface));
            write_u32(stream, uint32_t(vertices.size()));
            write_u32(stream, uint32_t(mesh.indices.size()));
            float3 lo{std::numeric_limits<float>::max()};
            float3 hi{-std::numeric_limits<float>::max()};
            for (const auto& v : vertices) {
                lo = min(lo, v.position);
                hi = max(hi, v.position);
            }
            write_vec3(stream, lo); write_vec3(stream, hi);
            for (size_t i = 0; i < vertices.size(); i += 3) {
                auto a = vertices[i], b = vertices[i + 1], c = vertices[i + 2];
                float3 ab = b.position - a.position;
                float3 ac = c.position - a.position;
                auto n = normalize(cross(ab, ac));
                float2 du = b.uv - a.uv;
                float2 dv = c.uv - a.uv;
                float determinant = du.x * dv.y - du.y * dv.x;
                float3 tangent = std::abs(determinant) > 1e-7f
                    ? (ab * dv.y - ac * du.y) / determinant : ab;
                tangent -= n * dot(n, tangent);
                if (length(tangent) < 1e-7f) tangent = normalize(cross(n, float3{0, 0, 1}));
                else tangent = normalize(tangent);
                float sign = determinant < 0 ? -1.f : 1.f;
                for (size_t j = i; j < i + 3; ++j) {
                    auto& v = vertices[j];
                    // 64 byte interleaved glTF vertex: P3/N3/T4/UV2/C4.
                    write_vec3(stream, v.position);
                    write_vec3(stream, n);
                    write_vec4(stream, tangent.x, tangent.y, tangent.z, sign);
                    write_f32(stream, v.uv.x);
                    write_f32(stream, v.uv.y);
                    write_vec4(stream, v.color.x, v.color.y, v.color.z, v.color.w);
                }
            }
            for (auto index : mesh.indices) {
                if (index >= vertices.size()) throw std::runtime_error("Bad index");
                write_u32(stream, index);
            }
            triangles += mesh.indices.size() / 3;
        }
        if (!stream) throw std::runtime_error("Write failed");
        std::cout << "Exported " << meshes.size() << " exact source meshes, "
                  << triangles << " triangles\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
