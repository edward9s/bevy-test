import bevy_pbr::render::{forward_io::{VertexOutput, FragmentOutput}, mesh_view_bindings::globals,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}};
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var reflection_map: texture_cube<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var reflection_sampler: sampler;
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, front);
    let a = in.uv.y*30.0 - globals.time*4.2 + sin(in.uv.x*15.0+globals.time)*0.4;
    let b = in.uv.x*25.0 + in.uv.y*19.0 - globals.time*3.1;
    let n = normalize(in.world_normal) * select(-1.0,1.0,front);
    let tangent = normalize(in.world_tangent.xyz);
    let bitangent = normalize(cross(n,tangent)) * in.world_tangent.w;
    pbr.N = normalize(tangent*cos(b)*0.12 + bitangent*cos(a)*0.18 + n);
    let foam = pow(max(0.0,sin(a)*sin(b)),12.0)*0.18;
    let fresnel = pow(1.0-clamp(dot(pbr.N,pbr.V),0.0,1.0),5.0);
    let opacity = mix(in.color.a*0.35,0.9,fresnel);
    var edge = 1.0;
    if in.color.a > 0.6 {
        edge = smoothstep(0.0,0.12,in.uv.x)*(1.0-smoothstep(0.88,1.0,in.uv.x));
        edge *= smoothstep(0.0,0.04,in.uv.y)*(1.0-smoothstep(0.96,1.0,in.uv.y));
    }
    pbr.material.base_color = vec4(mix(in.color.rgb,vec3(0.72,0.85,0.82),foam),opacity*edge);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr,apply_pbr_lighting(pbr));
    // The probe is centered at water origin + (0.75,0.8,1.9); box projection
    // matches the reference's 4 x 4 x 6 m one-time reflection volume.
    let local = in.world_position.xyz - vec3(4.25,0.8,5.4);
    let reflected = reflect(-pbr.V,pbr.N);
    let safe = select(vec3(0.00001),reflected,abs(reflected)>vec3(0.00001));
    let distances = (sign(safe)*vec3(2.0,2.0,3.0)-local)/safe;
    let distance = min(distances.x,min(distances.y,distances.z));
    let direction = local+reflected*max(distance,0.0);
    // Camera images are right-handed; cube lookup face coordinates reverse U.
    var cube_direction = direction;
    if abs(direction.x) >= max(abs(direction.y),abs(direction.z)) {
        cube_direction.z = -cube_direction.z;
    } else { cube_direction.x = -cube_direction.x; }
    let captured = textureSample(reflection_map,reflection_sampler,cube_direction).rgb;
    let reflectance = 0.02+0.98*fresnel;
    out.color = vec4(out.color.rgb + captured*reflectance*edge, out.color.a);
    return out;
}
