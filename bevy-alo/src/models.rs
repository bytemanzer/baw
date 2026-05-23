use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;

use bevy::camera::visibility::VisibilityRange;
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
use petro_alo::mesh::{Mesh as RawAloMesh, Shader as AloShader, ShaderParam, VertexFormat};
use petro_alo::skeleton::BillboardMode;

use regex::Regex;

use crate::components::{AloBillboard, AloBone, AloMesh};
use crate::labels::AloAssetLabel;
use crate::materials::{AloMaterial, AloMaterialExtension, TeamColorsMapping};
use crate::{ALO_TO_BEVY, AloError, alo_to_bevy};

pub struct AloModelsPlugin;

impl Plugin for AloModelsPlugin {
    fn build(&self, app: &mut App) {
        let team_colors_buffer = app
            .world()
            .resource::<TeamColorsMapping>()
            .team_colors_buffer
            .clone();
        app.register_asset_loader(AloLoader { team_colors_buffer });
    }
}

/// Loads ALO Models.
#[derive(Default, TypePath)]
struct AloLoader {
    /// Handle to the shader storage buffer containing the team colors.
    team_colors_buffer: Handle<ShaderStorageBuffer>,
}

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

        let mut lods = compute_lods(&file.meshes)?;
        let mut meshes = Vec::with_capacity(file.meshes.len());
        for (mesh_idx, mesh) in file.meshes.into_iter().enumerate() {
            println!("{}", mesh.name);
            let lod = lods.remove(&mesh.name);
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
                println!("    {:?}", submesh.vertex_format);
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
                if let Some(material) = build_submesh_material(
                    submesh.shader,
                    self.team_colors_buffer.clone(),
                    load_context,
                )? {
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
                    if let Some(ref lod) = lod {
                        submesh_builder.insert(lod.clone());
                    }
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
    println!("    {}", shader.filename);
    for (name, param) in &shader.params {
        println!("        {name} {param:?}");
    }
    match &*shader.filename {
        "MeshCollision.fx" | "MeshShadowVolume.fx" | "RSkinShadowVolume.fx" => return Ok(None),
        "MeshAlpha.fx" => {
            base.alpha_mode = AlphaMode::Blend;
        }
        "MeshAdditive.fx" | "MeshAdditiveVColor.fx" => {
            base.alpha_mode = AlphaMode::Add;
            base.unlit = true;
        }
        "Nebula.fx" => {
            base.alpha_mode = AlphaMode::Add;
            base.unlit = true;
            base.cull_mode = None;
        }
        "MeshBumpColorize.fx"
        | "MeshGlossColorize.fx"
        | "RSkinBumpColorize.fx"
        | "RSkinGlossColorize.fx" => {
            extension.default_team_color = Some(LinearRgba::WHITE);
        }
        "RSkinAlphaGloss.fx" => {
            base.alpha_mode = AlphaMode::Blend;
            extension.default_team_color = Some(LinearRgba::WHITE);
        }
        _ => {}
    }
    // Treat things as rough unless they use Gloss.
    base.perceptual_roughness = 1.0;
    if let Some(color) = expect_shader_param!(shader, "Color", Float4) {
        base.base_color = Srgba::from_f32_array(color).into()
    }
    if let Some(color) = expect_shader_param!(shader, "Colorization", Float4) {
        extension.default_team_color = Some(Srgba::from_f32_array(color).into());
    }
    if let Some(color) = expect_shader_param!(shader, "Diffuse", Float4) {
        let mut color = Srgba::from_f32_array(color);
        // When using team colorization we need to prevent overriding the texture map's alpha or
        // else no team coloring will occur.
        if extension.default_team_color.is_some() {
            color.alpha = 1.0;
        }
        base.base_color = color.into();
    }
    if let Some(shininess) = expect_shader_param!(shader, "Shininess", Float) {
        // To go from shininess (exponent) to roughness, we invert and square root (apparently?).
        extension.min_linear_roughness = Some((2.0 / (shininess + 2.0)).sqrt());
    }
    if let Some(color) = expect_shader_param!(shader, "DiffuseColor", Float3) {
        base.base_color = Srgba::from_f32_array_no_alpha(color).into();
    }
    if let Some(tex) = expect_shader_param!(shader, "BaseTexture", Texture) {
        let tex = load_alo_texture(load_context, tex, true);
        base.base_color_texture = Some(tex);
    }
    if let Some(tex) = expect_shader_param!(shader, "NormalTexture", Texture) {
        let tex = load_alo_texture(load_context, tex, false);
        base.normal_map_texture = Some(tex);
    }
    if let Some(tex) = expect_shader_param!(shader, "GlossTexture", Texture) {
        let tex = load_alo_texture(load_context, tex, false);
        extension.gloss_texture = Some(tex);
    }

    Ok(Some(ExtendedMaterial { base, extension }))
}

fn load_alo_texture(
    load_context: &mut LoadContext<'_>,
    mut tex_name: String,
    is_srgb: bool,
) -> Handle<Image> {
    let mut path = PathBuf::from("extracted");
    path.push("textures");
    tex_name.make_ascii_lowercase();
    path.push(tex_name);
    path.set_extension("dds");
    load_context
        .loader()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = is_srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path)
}

/// Compute level of detail ranges for the given mesh.
fn compute_lods(meshes: &[RawAloMesh]) -> Result<HashMap<String, VisibilityRange>, AloError> {
    static LOD_FINDER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.*)LOD([0-9]+)$").unwrap());

    let mut lod_stack = HashMap::<&str, Vec<u32>>::new();
    for mesh in meshes {
        let (key, lod) = match LOD_FINDER.captures(&mesh.name) {
            None => continue,
            Some(matches) => (
                matches.get(1).unwrap().as_str(),
                matches
                    .get(2)
                    .unwrap()
                    .as_str()
                    .parse::<u32>()
                    .map_err(|_| {
                        AloError::InvalidLodNumber(matches.get(2).unwrap().as_str().to_owned())
                    })?,
            ),
        };
        lod_stack.entry(key).or_default().push(lod);
    }

    let mut result = HashMap::new();

    for (key, mut lods) in lod_stack {
        if lods.len() < 2 {
            continue;
        }
        lods.sort();
        let mut prev_end = None;
        for (idx, &lod) in lods.iter().rev().enumerate() {
            let start_margin = match prev_end {
                Some(prev_end) => prev_end,
                None => 0f32..0f32,
            };
            let end_margin = if idx == lods.len() - 1 {
                f32::INFINITY..f32::INFINITY
            } else {
                const RANGE_SCALE_BASE: f32 = 300.0;
                RANGE_SCALE_BASE.powf(idx as f32 * 0.15 + 1.0)
                    ..RANGE_SCALE_BASE.powf(idx as f32 * 0.15 + 1.1)
            };
            prev_end = Some(end_margin.clone());
            let range = VisibilityRange {
                start_margin,
                end_margin,
                ..default()
            };
            if let Some(_) = result.insert(format!("{key}LOD{lod}"), range.clone()) {
                return Err(AloError::DuplicateLod {
                    num: lod,
                    basename: key.to_owned(),
                });
            }
        }
    }

    Ok(result)
}
