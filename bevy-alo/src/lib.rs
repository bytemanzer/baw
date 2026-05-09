use std::fmt;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use bevy::transform::components::Transform;
use bytemuck::cast_vec;
pub use error::AloError;
use petro_alo::AloFile;
use petro_alo::skeleton::BillboardMode;

mod error;

/// Plugin which adds the ALO asset loader.
pub struct AloPlugin;

impl Plugin for AloPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(AloLoader)
            .register_type::<AloBone>()
            .register_type::<AloBillboard>();
    }
}

/// Loads ALO Models.
#[derive(Default, TypePath)]
pub struct AloLoader;

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
            inverse_bindposes.push(ALO_TO_BEVY * bone.local_to_global.inverse() * bevy_to_alo);
        }

        let mut meshes = Vec::with_capacity(file.meshes.len());
        for (mesh_idx, mesh) in file.meshes.into_iter().enumerate() {
            let mesh_id = world
                .spawn((
                    AloMesh,
                    Transform::IDENTITY,
                    Name::new(mesh.name),
                    if mesh.hidden {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    },
                ))
                .id();
            for (submesh_idx, mut submesh) in mesh.submeshes.into_iter().enumerate() {
                alo_to_bevy(&mut submesh.positions);
                alo_to_bevy(&mut submesh.normals);
                alo_to_bevy(&mut submesh.tangents);
                alo_to_bevy(&mut submesh.binormals);

                let mut msh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::RENDER_WORLD,
                );
                msh.insert_indices(Indices::U16(cast_vec(submesh.primitives)));
                msh.insert_attribute(Mesh::ATTRIBUTE_POSITION, submesh.positions);
                match &*submesh.vertex_format {
                    "alD3dVertN" => {
                        msh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, submesh.normals);
                    }
                    "alD3dVertNU2" => {
                        msh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, submesh.normals);
                        let [uv0, uv1, _, _] = submesh.tex_coords;
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
                    }
                    "alD3dVertNU2C" => {
                        msh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, submesh.normals);
                        let [uv0, uv1, _, _] = submesh.tex_coords;
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
                        msh.insert_attribute(Mesh::ATTRIBUTE_COLOR, submesh.colors);
                    }
                    "alD3dVertNU2U3U3" => {
                        let tangents = combine_tangents(
                            &submesh.normals,
                            &submesh.tangents,
                            &submesh.binormals,
                        );
                        msh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, submesh.normals);
                        let [uv0, uv1, _, _] = submesh.tex_coords;
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
                        msh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
                        msh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
                    }
                    _ => return Err(AloError::UnsupportedVertextFormat(submesh.vertex_format)),
                }
                let msh = load_context.add_labeled_asset(
                    AloAssetLabel::Submesh {
                        mesh_idx,
                        submesh_idx,
                    }
                    .to_string(),
                    msh,
                );
                let color = load_context.add_labeled_asset(
                    AloAssetLabel::SubmeshMaterial {
                        mesh_idx,
                        submesh_idx,
                    }
                    .to_string(),
                    StandardMaterial {
                        base_color: Srgba::WHITE.into(),
                        ..Default::default()
                    },
                );
                world.spawn((
                    Mesh3d(msh),
                    MeshMaterial3d(color),
                    Visibility::Inherited,
                    ChildOf(mesh_id),
                ));
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
fn alo_to_bevy(vals: &mut [[f32; 3]]) {
    for point in vals {
        *point = ALO_TO_BEVY
            .transform_vector3(Vec3::from_array(*point))
            .to_array();
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

/// Label for sub-assets of an ALO model.
#[derive(Clone, Debug)]
pub enum AloAssetLabel {
    Submesh { mesh_idx: usize, submesh_idx: usize },
    SubmeshMaterial { mesh_idx: usize, submesh_idx: usize },
}

impl fmt::Display for AloAssetLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Submesh {
                mesh_idx,
                submesh_idx,
            } => {
                write!(f, "Mesh{mesh_idx}Submesh{submesh_idx}")?;
            }
            Self::SubmeshMaterial {
                mesh_idx,
                submesh_idx,
            } => {
                write!(f, "Mesh{mesh_idx}Submesh{submesh_idx}Material")?;
            }
        }
        Ok(())
    }
}

/// Marker component for entities that are the bones of an Alo skeleton.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AloBone;

/// Holds the [`BillboardMode`] of an ALO bone.
#[derive(Component, Reflect, Clone)]
#[reflect(opaque)]
pub struct AloBillboard(pub BillboardMode);

/// Marker component for entities that are the "meshes" of an Alo file.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AloMesh;
