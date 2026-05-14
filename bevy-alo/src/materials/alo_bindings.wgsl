#define_import_path bevy_alo::alo_bindings

struct AloMaterialExtensionUniform {
    default_team_color: vec4<f32>,
}

// Ensure you use Bevy's template syntax #{MATERIAL_BIND_GROUP}
// for ExtendedMaterial mappings!
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> alo_extension: AloMaterialExtensionUniform;

@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var<storage, read> team_colors: array<vec4<f32>; 32>;
