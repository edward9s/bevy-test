import bevy_pbr::render::{forward_io::{Vertex, VertexOutput, decompress_vertex}, mesh_functions,
    mesh_view_bindings::globals, view_transformations::position_world_to_clip};
@vertex
fn vertex(input: Vertex) -> VertexOutput {
    var v = decompress_vertex(input, input.instance_index);
    if v.color.a > 0.6 {
        v.position.y += 0.012 * sin(v.uv.y * 25.0 - globals.time * 4.0 + v.uv.x * 8.0);
    } else {
        // Expand the tube radially, retaining its original arc and UVs.
        let t = v.uv.y;
        let center = vec3(t*0.75, 1.15+t*0.55-1.8*t*t, t*1.45);
        v.position = center + (v.position-center) * (1.0 + 0.125*sin(globals.time*8.0 + floor(t*24.0)));
    }
    let model = mesh_functions::get_world_from_local(input.instance_index);
    var out: VertexOutput;
    out.world_position = mesh_functions::mesh_position_local_to_world(model, vec4(v.position,1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal,input.instance_index);
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(model,v.tangent,input.instance_index);
    out.uv = v.uv;
    out.color = v.color;
    @if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = input.instance_index;
    return out;
}
