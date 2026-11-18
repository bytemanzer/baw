struct AloMaterialExtensionUniform {
    default_team_color: vec4<f32>,
    min_linear_roughness: f32,
    _pad_0_: f32,
    _pad_1_: f32,
    _pad_2_: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> alo_extension: AloMaterialExtensionUniform;

@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<storage, read> team_colors: array<vec4<f32>, 32>;

@group(constants::MATERIAL_BIND_GROUP) @binding(102) var gloss_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var gloss_sampler: sampler;
