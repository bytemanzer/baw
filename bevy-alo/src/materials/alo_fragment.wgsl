#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_types,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, alpha_discard},
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}

#import bevy_alo::alo_io::AloVertexOutput;

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#endif // OIT_ENABLED

#ifdef FORWARD_DECAL
#import bevy_pbr::decal::forward::get_forward_decal_info
#endif


#ifdef ALO_GLOSS
#import bevy_alo::alo_bindings::{gloss_texture, gloss_sampler, alo_extension};
#endif

#ifdef ALO_TEAM_COLOR
fn blend_team_color(base_color: vec4<f32>, team_color: vec4<f32>) -> vec4<f32> {
    let colorized = base_color * team_color;
    let mixed = mix(base_color.xyz, colorized.xyz, base_color.w);
    return vec4(mixed, base_color.w);
}
#endif

#ifdef ALO_GLOSS
fn roughness_from_legacy_gloss(uv: vec2<f32>) -> f32 {
    let raw_gloss = textureSample(gloss_texture, gloss_sampler, uv).r;
    let linear_roughness = mix(1.0, alo_extension.min_linear_roughness, raw_gloss);
    return clamp(sqrt(linear_roughness), 0.2, 1.0);
}
#endif ALO_GLOSS

@fragment
fn fragment(
    vertex_output: AloVertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in: VertexOutput;
    in.position = vertex_output.position;
    in.world_position = vertex_output.world_position;
    in.world_normal = vertex_output.world_normal;
#ifdef VERTEX_UVS_A
    in.uv = vertex_output.uv;
#endif
#ifdef VERTEX_UVS_B
    in.uv_b = vertex_output.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    in.world_tangent = vertex_output.world_tangent;
#endif
#ifdef VERTEX_COLORS
    in.color = vertex_output.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    in.instance_index = vertex_output.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    in.visibility_range_dither = vertex_output.visibility_range_dither;
#endif

    // If we're in the crossfade section of a visibility range, conditionally
    // discard the fragment according to the visibility pattern.
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif

#ifdef FORWARD_DECAL
    let forward_decal_info = get_forward_decal_info(in);
    in.world_position = forward_decal_info.world_position;
    in.uv = forward_decal_info.uv;
#endif

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // ALO CUSTOMIZATION
#ifdef ALO_TEAM_COLOR
    pbr_input.material.base_color =
        blend_team_color(pbr_input.material.base_color, vertex_output.team_color);
#endif

#ifdef ALO_GLOSS
    pbr_input.material.perceptual_roughness = roughness_from_legacy_gloss(in.uv);
#endif
    // END ALO CUSTOMIZATION


    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // clustered decals
    apply_decals(&pbr_input);

    // in forward mode, we calculate the lit color immediately, and then apply some post-lighting effects here.
    // in deferred mode the lit color and these effects will be calculated in the deferred lighting shader
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        // The fragments will only be drawn during the oit resolve pass.
        oit_draw(in.position, out.color);
        discard;
    }
#endif // OIT_ENABLED

#ifdef FORWARD_DECAL
        out.color.a = min(forward_decal_info.alpha, out.color.a);
#endif

        return out;
}
