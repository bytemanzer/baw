use bevy::asset::{Asset, Handle, embedded_asset};
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
use bevy::shader::load_shader_library;
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
    /// Minimum roughness, converted from "shininess". This is in linear, not perceptual space.
    /// This sets the limit on how shiny the "gloss" texture can make the material.
    pub min_linear_roughness: Option<f32>,
    /// Gloss texture for the model.
    #[texture(102)]
    #[sampler(103)]
    pub gloss_texture: Option<Handle<Image>>,
}

impl AloMaterialExtension {
    /// Create a new, empty extension material.
    pub(crate) fn new(team_colors: Handle<ShaderStorageBuffer>) -> Self {
        Self {
            default_team_color: None,
            team_colors,
            min_linear_roughness: None,
            gloss_texture: None,
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
            if key
                .bind_group_data
                .contains(AloMaterialExtensionKey::GLOSS)
            {
                fragment.shader_defs.push("ALO_GLOSS".into());
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
        const GLOSS = 0x2;
    }
}

impl From<&AloMaterialExtension> for AloMaterialExtensionKey {
    fn from(extension: &AloMaterialExtension) -> Self {
        let mut key = Self::empty();
        key.set(Self::TEAM_COLOR, extension.default_team_color.is_some());
        key.set(
            Self::GLOSS,
            extension.min_linear_roughness.is_some() || extension.gloss_texture.is_some(),
        );
        key
    }
}

#[derive(Clone, Default, ShaderType)]
struct AloMaterialExtensionUniform {
    default_team_color: LinearRgba,
    min_linear_roughness: f32,
    _pad_0: f32,
    _pad_1: f32,
    _pad_2: f32,
}

impl From<&AloMaterialExtension> for AloMaterialExtensionUniform {
    fn from(value: &AloMaterialExtension) -> Self {
        /// Equivalent to a Shininess of 32.
        const DEFAULT_MIN_ROUGHNESS: f32 = 0.24253563;
        Self {
            default_team_color: value.default_team_color.unwrap_or(LinearRgba::WHITE),
            min_linear_roughness: value.min_linear_roughness.unwrap_or(DEFAULT_MIN_ROUGHNESS),
            _pad_0: 0.0,
            _pad_1: 0.0,
            _pad_2: 0.0,
        }
    }
}
