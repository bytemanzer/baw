use bevy::asset::{Asset, Handle, embedded_asset};
use bevy::shader::load_shader_library;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;
use bitflags::bitflags;

pub struct AloMaterialsPlugin;

impl Plugin for AloMaterialsPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "alo_bindings.wgsl");
        load_shader_library!(app, "alo_io.wgsl");
        embedded_asset!(app, "alo_vertex.wgsl");
        embedded_asset!(app, "alo_fragment.wgsl");
        let team_colors_buffer =
            app.world_mut()
                .add_asset(ShaderStorageBuffer::from(vec![[1.0f32, 1.0, 1.0, 0.0]; 32]));
        app.add_plugins(MaterialPlugin::<AloMaterial>::default())
            .register_type::<AloMaterialExtension>()
            .insert_resource(TeamColorsMapping { team_colors_buffer });
    }
}

/// Internal resource which stores the handle to the team colors buffer.
#[derive(Resource)]
struct TeamColorsMapping {
    team_colors_buffer: Handle<ShaderStorageBuffer>,
}

/// Adds material properties needed to properly support ALO materials.
pub type AloMaterial = ExtendedMaterial<StandardMaterial, AloMaterialExtension>;

/// Additional properties added to
#[derive(AsBindGroup, Reflect, Debug, Clone, Asset)]
#[bind_group_data(AloMaterialExtensionKey)]
#[uniform(100, AloMaterialExtensionUniform)]
pub struct AloMaterialExtension {
    /// The default team color for this material.
    pub default_team_color: Option<LinearRgba>,
    /// The buffer of possible team colors.
    #[storage(101, read_only)]
    team_colors: Handle<ShaderStorageBuffer>,
}

impl AloMaterialExtension {
    /// Create a new, empty extension material.
    pub(crate) fn new(team_colors: Handle<ShaderStorageBuffer>) -> Self {
        Self {
            default_team_color: None,
            team_colors,
        }
    }
}

impl MaterialExtension for AloMaterialExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://bevy_alo/materials/alo_vertex.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_alo/materials/alo_fragment.wgsl".into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key
            .bind_group_data
            .contains(AloMaterialExtensionKey::TEAM_COLOR)
        {
            descriptor.vertex.shader_defs.push("ALO_TEAM_COLOR".into());
        }
        if let Some(ref mut fragment) = descriptor.fragment {
            if key
                .bind_group_data
                .contains(AloMaterialExtensionKey::TEAM_COLOR)
            {
                fragment.shader_defs.push("ALO_TEAM_COLOR".into());
            }
        }
        Ok(())
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AloMaterialExtensionKey: u32 {
        const TEAM_COLOR = 0x1;
    }
}

impl From<&AloMaterialExtension> for AloMaterialExtensionKey {
    fn from(extension: &AloMaterialExtension) -> Self {
        let mut key = Self::empty();
        key.set(Self::TEAM_COLOR, extension.default_team_color.is_some());
        key
    }
}

#[derive(Clone, Default, ShaderType)]
struct AloMaterialExtensionUniform {
    default_team_color: LinearRgba,
}

impl From<&AloMaterialExtension> for AloMaterialExtensionUniform {
    fn from(value: &AloMaterialExtension) -> Self {
        Self {
            default_team_color: value.default_team_color.unwrap_or(LinearRgba::WHITE),
        }
    }
}
