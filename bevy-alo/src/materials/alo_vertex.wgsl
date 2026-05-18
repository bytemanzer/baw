#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::morph,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

#import bevy_alo::alo_io::AloVertexOutput;
#import bevy_alo::alo_bindings::alo_extension;

#ifdef MORPH_TARGETS
fn morph_vertex(vertex_in: Vertex) -> Vertex {
    var vertex = vertex_in;
    let first_vertex = mesh[vertex.instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;

    let weight_count = bevy_pbr::morph::layer_count();
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = bevy_pbr::morph::weight_at(i);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph(vertex_index, bevy_pbr::morph::position_offset, i);
#ifdef VERTEX_NORMALS
        vertex.normal += weight * morph(vertex_index, bevy_pbr::morph::normal_offset, i);
#endif
#ifdef VERTEX_TANGENTS
        vertex.tangent += vec4(weight * morph(vertex_index, bevy_pbr::morph::tangent_offset, i), 0.0);
#endif
    }
    return vertex;
}
#endif

#ifdef ALO_TEAM_COLOR
#import bevy_alo::alo_bindings::team_colors;

fn get_team_color(instance_index: u32) -> vec4<f32> {
    let tag = mesh_functions::get_tag(instance_index);
    let team_color = team_colors[min(tag, 31)];
    return mix(alo_extension.default_team_color, team_color, team_color.w);
}
#endif

@vertex
fn vertex(vertex_no_morph: Vertex) -> AloVertexOutput {
    var out: VertexOutput;

#ifdef MORPH_TARGETS
    var vertex = morph_vertex(vertex_no_morph);
#else
    var vertex = vertex_no_morph;
#endif

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);

#ifdef SKINNED
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex_no_morph.instance_index
    );
#else
    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416 .
    var world_from_local = mesh_world_from_local;
#endif

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        vertex_no_morph.instance_index
    );
#endif
#endif

#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
        // See https://github.com/gfx-rs/naga/issues/2416
        vertex_no_morph.instance_index
    );
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    // Use vertex_no_morph.instance_index instead of vertex.instance_index to work around a wgpu dx12 bug.
    // See https://github.com/gfx-rs/naga/issues/2416
    out.instance_index = vertex_no_morph.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);
#endif

    var alo_out: AloVertexOutput;
    alo_out.position = out.position;
    alo_out.world_position = out.world_position;
    alo_out.world_normal = out.world_normal;
#ifdef VERTEX_UVS_A
    alo_out.uv = out.uv;
#endif
#ifdef VERTEX_UVS_B
    alo_out.uv_b = out.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    alo_out.world_tangent = out.world_tangent;
#endif
#ifdef VERTEX_COLORS
    alo_out.color = out.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    alo_out.instance_index = out.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    alo_out.visibility_range_dither = out.visibility_range_dither;
#endif

#ifdef ALO_TEAM_COLOR
    alo_out.team_color = get_team_color(vertex_no_morph.instance_index);
#endif

    return alo_out;
}
