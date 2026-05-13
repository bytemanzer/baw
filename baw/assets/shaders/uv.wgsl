#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
}

@fragment
fn fragment(
    mesh: VertexOutput,
) -> FragmentOutput {
    var out: FragmentOutput;
    out.color = vec4<f32>(mesh.uv, -mesh.uv.y, 1.0);
    return out;
}
