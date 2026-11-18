import bevy_pbr::{
    render::{
        forward_io::{VertexOutput, FragmentOutput},
        mesh_functions,
        pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, alpha_discard},
        pbr_fragment::pbr_input_from_standard_material,
        pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    },
    decal::clustered::apply_decals,
};

import bevy_alo::materials::alo_bindings::alo_extension;

@if(ALO_TEAM_COLOR)
import bevy_alo::materials::alo_bindings::team_colors;

@if(ALO_GLOSS)
import bevy_alo::materials::alo_bindings::{gloss_texture, gloss_sampler};

@if(ALO_TEAM_COLOR)
fn get_team_color(instance_index: u32) -> vec4<f32> {
    let tag = mesh_functions::get_tag(instance_index);
    let team_color = team_colors[min(tag, 31u)];
    return mix(alo_extension.default_team_color, team_color, team_color.w);
}

@if(ALO_TEAM_COLOR)
fn blend_team_color(base_color: vec4<f32>, team_color: vec4<f32>) -> vec4<f32> {
    let mixed = mix(base_color.xyz, (base_color * team_color).xyz, base_color.w);
    return vec4(mixed, 1.0);
}

@if(ALO_GLOSS)
fn roughness_from_legacy_gloss(uv: vec2<f32>) -> f32 {
    let raw_gloss = textureSample(gloss_texture, gloss_sampler, uv).r;
    let linear_roughness = mix(1.0, alo_extension.min_linear_roughness, raw_gloss);
    return clamp(sqrt(linear_roughness), 0.2, 1.0);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

@if(ALO_TEAM_COLOR) {
    let team_color = get_team_color(in.instance_index);
    pbr_input.material.base_color =
        blend_team_color(pbr_input.material.base_color, team_color);
}

@if(ALO_GLOSS)
    pbr_input.material.perceptual_roughness = roughness_from_legacy_gloss(in.uv);

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );

    apply_decals(&pbr_input);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
