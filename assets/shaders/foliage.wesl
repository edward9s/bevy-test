import bevy_pbr::render::{mesh_functions, view_transformations::position_world_to_clip};
// Both passes use the same engine clock, at their respective layout bindings.
@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{Vertex, VertexOutput, decompress_vertex};
@if(PREPASS_PIPELINE)
import bevy_render::globals::Globals;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::forward_io::{Vertex, VertexOutput, decompress_vertex};
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::mesh_view_bindings::globals;

@if(PREPASS_PIPELINE)
@group(0) @binding(1) var<uniform> globals: Globals;
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> canopy: u32;

@vertex
fn vertex(input: Vertex) -> VertexOutput {
    var v = decompress_vertex(input, input.instance_index);
    let phase = v.position.x * 0.71 + v.position.z * 0.49;
    let wind = sin(globals.time * 1.2 + phase) + 0.35 * sin(globals.time * 2.6 + phase * 2.0);
    let weight = select(v.color.a, 1.0, canopy == 1u);
    v.position.x += 0.06 * wind * weight;
    v.position.z += 0.025 * wind * weight;
    let model = mesh_functions::get_world_from_local(input.instance_index);
    var out: VertexOutput;
    out.world_position = mesh_functions::mesh_position_local_to_world(model, vec4(v.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    @if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
        out.unclipped_depth = out.position.z;
        out.position.z = min(out.position.z, 1.0);
    }
    @if(!PREPASS_PIPELINE || NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, input.instance_index);
    @if(VERTEX_TANGENTS && (!PREPASS_PIPELINE || NORMAL_PREPASS_OR_DEFERRED_PREPASS))
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(model, v.tangent, input.instance_index);
    out.uv = v.uv;
    out.color = vec4(v.color.rgb, 1.0);
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = input.instance_index;
    return out;
}
