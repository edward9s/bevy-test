import bevy_pbr::render::{forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing}};
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> canopy: u32;
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, front);
    if canopy == 0u {
        let vein = smoothstep(0.015, 0.045, abs(in.uv.x - 0.5));
        let ribs = 0.94 + 0.06 * sin((in.uv.y + abs(in.uv.x - 0.5) * 0.6) * 100.0);
        pbr.material.base_color = vec4(pbr.material.base_color.rgb * mix(1.25, ribs, vein), 1.0);
    }
    pbr.material.base_color = alpha_discard(pbr.material.flags, pbr.material.alpha_cutoff, pbr.material.base_color);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr, apply_pbr_lighting(pbr));
    return out;
}
