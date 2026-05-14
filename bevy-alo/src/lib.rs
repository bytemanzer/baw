use std::path::PathBuf;

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology, VertexAttributeValues};
use bevy::render::storage::ShaderStorageBuffer;
use bevy::transform::components::Transform;
use bytemuck::cast_vec;
use petro_alo::AloFile;
use petro_alo::mesh::{Shader as AloShader, ShaderParam, VertexFormat};
use petro_alo::skeleton::BillboardMode;

pub use error::AloError;

use crate::components::{AloBillboard, AloBone, AloComponentsPlugin, AloMesh};
use crate::labels::AloAssetLabel;
use crate::materials::{AloMaterial, AloMaterialExtension, AloMaterialsPlugin};

pub mod components;
mod error;
pub mod labels;
pub mod materials;

/// Plugin which adds the ALO asset loader.
pub struct AloPlugin;

impl Plugin for AloPlugin {
    fn build(&self, app: &mut App) {
        let team_colors =
            app.world_mut()
                .add_asset(ShaderStorageBuffer::from(vec![[1.0, 0.0, 1.0, 0.0]; 32]));
        app.add_plugins((AloMaterialsPlugin, AloComponentsPlugin));
        app.register_asset_loader(AloLoader { team_colors });
    }
}

/// Loads ALO Models.
#[derive(Default, TypePath)]
pub struct AloLoader {
    /// Handle to the shader storage buffer containing the team colors.
    team_colors: Handle<ShaderStorageBuffer>,
}

const ALO_TO_BEVY: Mat4 =
    Mat4::from_cols(Vec4::X, Vec4::new(0.0, 0.0, -1.0, 0.0), Vec4::Y, Vec4::W);

impl AssetLoader for AloLoader {
    type Asset = Scene;
    type Error = AloError;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let file = AloFile::parse(&bytes)?;
        let mut world = World::default();

        let bevy_to_alo = ALO_TO_BEVY.inverse();
        let mut inverse_bindposes = Vec::with_capacity(file.skeleton.bones.len());
        let mut bones = Vec::with_capacity(file.skeleton.bones.len());
        for bone in file.skeleton.bones {
            let mut bone_builder = world.spawn((
                AloBone,
                // To convert a matrix from alo to bevy, we create a matrix which first transforms
                // from bevy space to alo space, then performs the transform in alo space, then
                // transforms back to bevy space.
                Transform::from_matrix(ALO_TO_BEVY * bone.local_to_parent * bevy_to_alo),
                Name::new(bone.name),
                if bone.visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            if bone.billboard != BillboardMode::Disable {
                bone_builder.insert(AloBillboard(bone.billboard));
            }
            if let Some(parent) = bone.parent {
                bone_builder.insert(ChildOf(bones[parent]));
            }
            bones.push(bone_builder.id());
            inverse_bindposes.push((ALO_TO_BEVY * bone.local_to_global * bevy_to_alo).inverse());
        }

        let mut meshes = Vec::with_capacity(file.meshes.len());
        for (mesh_idx, mesh) in file.meshes.into_iter().enumerate() {
            let mesh_id = {
                let mut mesh_builder = world.spawn((
                    AloMesh,
                    Transform::IDENTITY,
                    Name::new(mesh.name),
                    if mesh.hidden {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    },
                ));
                if let Some(bone_idx) = mesh.parent_bone {
                    mesh_builder.insert(ChildOf(bones[bone_idx]));
                }
                mesh_builder.id()
            };
            for (submesh_idx, submesh) in mesh.submeshes.into_iter().enumerate() {
                let mut msh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::RENDER_WORLD,
                );
                msh.insert_indices(Indices::U16(cast_vec(submesh.primitives)));
                if submesh.vertex_format.contains(VertexFormat::Positions) {
                    msh.insert_attribute(Mesh::ATTRIBUTE_POSITION, alo_to_bevy(submesh.positions));
                }
                if submesh
                    .vertex_format
                    .intersects(VertexFormat::Normals | VertexFormat::TangentBinormals)
                {
                    let normals = alo_to_bevy(submesh.normals);
                    if submesh
                        .vertex_format
                        .contains(VertexFormat::TangentBinormals)
                    {
                        let tangents = combine_tangents(
                            &normals,
                            &alo_to_bevy(submesh.tangents),
                            &alo_to_bevy(submesh.binormals),
                        );
                        msh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
                    }
                    if submesh.vertex_format.contains(VertexFormat::Normals) {
                        msh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                    }
                }
                if submesh.vertex_format.contains(VertexFormat::TexCoords) {
                    let [uv0, uv1, _, _] = submesh.tex_coords;
                    msh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
                    msh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
                }
                if submesh.vertex_format.contains(VertexFormat::Colors) {
                    msh.insert_attribute(Mesh::ATTRIBUTE_COLOR, submesh.colors);
                }
                if submesh.vertex_format.contains(VertexFormat::Bones) {
                    msh.insert_attribute(
                        Mesh::ATTRIBUTE_JOINT_INDEX,
                        VertexAttributeValues::Uint16x4(submesh.bone_indices),
                    );
                    msh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, submesh.bone_weights);
                }

                let msh = load_context.add_labeled_asset(
                    AloAssetLabel::Submesh {
                        mesh_idx,
                        submesh_idx,
                    }
                    .to_string(),
                    msh,
                );
                // Build an entity with a visible mesh if a material exists for it.
                if let Some(material) =
                    build_submesh_material(submesh.shader, self.team_colors.clone(), load_context)?
                {
                    let material_label = AloAssetLabel::SubmeshMaterial {
                        mesh_idx,
                        submesh_idx,
                    };
                    let material =
                        load_context.add_labeled_asset(material_label.to_string(), material);
                    let mut submesh_builder = world.spawn((
                        Mesh3d(msh),
                        MeshMaterial3d(material),
                        MeshTag(0),
                        Transform::IDENTITY,
                        ChildOf(mesh_id),
                        Visibility::Inherited,
                    ));
                    if submesh.vertex_format.contains(VertexFormat::Bones) {
                        let mut submesh_inverse_bindposes =
                            Vec::with_capacity(submesh.bone_mappings.len());
                        let mut joints = Vec::with_capacity(submesh.bone_mappings.len());
                        for mapping in submesh.bone_mappings {
                            submesh_inverse_bindposes.push(inverse_bindposes[mapping]);
                            joints.push(bones[mapping]);
                        }
                        let inverse_bindposes = load_context.add_labeled_asset(
                            AloAssetLabel::SubmeshInverseBindposes {
                                mesh_idx,
                                submesh_idx,
                            }
                            .to_string(),
                            SkinnedMeshInverseBindposes::from(submesh_inverse_bindposes),
                        );
                        submesh_builder.insert(SkinnedMesh {
                            inverse_bindposes,
                            joints,
                        });
                    }
                }
            }
            meshes.push(mesh_id);
        }

        Ok(Scene { world })
    }

    fn extensions(&self) -> &[&str] {
        &["alo"]
    }
}

/// Transform a slice of ALO vectors to Bevy space. Since this transform does no translation, this
/// works on both position vectors and direction vectors.
fn alo_to_bevy(mut vals: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    for point in &mut vals {
        *point = ALO_TO_BEVY
            .transform_vector3(Vec3::from_array(*point))
            .to_array();
    }
    vals
}

fn combine_tangents(norms: &[[f32; 3]], tans: &[[f32; 3]], binorms: &[[f32; 3]]) -> Vec<[f32; 4]> {
    assert!(norms.len() == tans.len());
    assert!(norms.len() == binorms.len());
    let mut res = Vec::with_capacity(norms.len());
    for ((&norm, &tan), &binorm) in norms.iter().zip(tans).zip(binorms) {
        res.push(combine_tangent(norm, tan, binorm));
    }
    res
}

fn combine_tangent(norm: [f32; 3], tan: [f32; 3], binorm: [f32; 3]) -> [f32; 4] {
    let w = Vec3::from_array(norm)
        .cross(Vec3::from_array(tan))
        .dot(Vec3::from_array(binorm))
        .signum();
    [tan[0], tan[1], tan[2], w]
}

macro_rules! expect_shader_param {
    ($shader:ident, $name:literal, $ty:ident) => {
        match $shader.params.remove($name) {
            Some(ShaderParam::$ty(value)) => Some(value),
            Some(other) => {
                return Err(AloError::UnexpectedShaderValueType {
                    param: $name,
                    value: other,
                    expected: stringify!($ty),
                })
            }
            None => None,
        }
    };
}

/// Convert an ALO shader to a Bevy material (within reason).
fn build_submesh_material(
    mut shader: AloShader,
    team_colors: Handle<ShaderStorageBuffer>,
    load_context: &mut LoadContext<'_>,
) -> Result<Option<AloMaterial>, AloError> {
    let mut base = StandardMaterial::default();
    let mut extension = AloMaterialExtension::new(team_colors);

    match &*shader.filename {
        "MeshCollision.fx" | "MeshShadowVolume.fx" | "RSkinShadowVolume.fx" => return Ok(None),
        "MeshAdditive.fx" => base.alpha_mode = AlphaMode::Add,
        "MeshAlpha.fx" => base.alpha_mode = AlphaMode::Blend,
        "MeshBumpColorize.fx"
        | "MeshGlossColorize.fx"
        | "RSkinAlphaGloss.fx"
        | "RSkinBumpColorize.fx"
        | "RSkinGlossColorize.fx" => {
            extension.default_team_color = Some(LinearRgba::WHITE);
        }
        _ => {}
    }
    if let Some(color) = expect_shader_param!(shader, "Color", Float4) {
        base.base_color = Srgba::from_f32_array(color).into()
    }
    if let Some(color) = expect_shader_param!(shader, "Colorization", Float4) {
        extension.default_team_color = Some(Srgba::from_f32_array(color).into());
    }
    if let Some([r, g, b, diffuse]) = expect_shader_param!(shader, "Diffuse", Float4) {
        base.base_color = Srgba::from_f32_array_no_alpha([r, g, b]).into();
        base.diffuse_transmission = diffuse;
    }
    if let Some(color) = expect_shader_param!(shader, "DiffuseColor", Float3) {
        base.base_color = Srgba::from_f32_array_no_alpha(color).into();
    }
    if let Some(mut tex) = expect_shader_param!(shader, "BaseTexture", Texture) {
        let mut path = PathBuf::from("extracted");
        path.push("textures");
        tex.make_ascii_lowercase();
        path.push(tex);
        path.set_extension("dds");
        let tex = load_context
            .loader()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            })
            .load(path);
        base.base_color_texture = Some(tex);
    }

    Ok(Some(ExtendedMaterial { base, extension }))
}
