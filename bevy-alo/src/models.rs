use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::PathBuf;

use baw_scene_util::{SceneTypeId, scene_type_id, with_template};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, AssetPath, LoadContext, RenderAssetUsages};
use bevy::camera::visibility::VisibilityRange;
use bevy::ecs::template::{EntityTemplate, SceneEntityReference};
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy::scene::macro_utils::CallCounter;
use bevy::scene::{
    NameEntityReference, ResolveContext, ResolveSceneError, ResolvedScene, SceneDependencies,
    ScenePatch,
};
use bevy::transform::components::Transform;
use bytemuck::cast_vec;
use petro_alo::AloFile;
use petro_alo::alternatives::{Lod, LodGroup};
use petro_alo::mesh::{
    Mesh as RawAloMesh, Shader as AloShader, ShaderParam, Submesh, SubmeshVertexData, VertexFormat,
};
use petro_alo::skeleton::{BillboardMode, Skeleton};
use serde::{Deserialize, Serialize};

use crate::components::{AloBone, AloFaceCamera, AloInitialVisibility, AloMesh};
use crate::labels::AloAssetLabel;
use crate::materials::{AloMaterial, AloMaterialExtension, TeamColorsMapping};
use crate::{ALO_TO_BEVY, AloError, alo_to_bevy};

#[derive(Default, Clone)]
pub struct AloModelsPlugin {
    _private: (),
}

impl AloModelsPlugin {
    /// Create a new AloModelsPlugin with no path prefixes set.
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Plugin for AloModelsPlugin {
    fn build(&self, app: &mut App) {
        let team_colors_buffer = app
            .world()
            .resource::<TeamColorsMapping>()
            .team_colors_buffer
            .clone();
        app.register_asset_loader(AloLoader {
            team_colors_buffer,
            lod_end_ranges: {
                let mut ranges = BTreeMap::new();
                // As far as I know real models only use LODs 0-2.
                ranges.insert(Lod(0), f32::INFINITY..f32::INFINITY);
                ranges.insert(Lod(1), 950.0..1000.0);
                ranges.insert(Lod(2), 290.0..300.0);
                ranges.insert(Lod(3), 100.0..110.0);
                ranges.insert(Lod(4), 45.0..50.0);
                ranges.insert(Lod(5), 9.0..10.0);
                ranges
            },
        });
    }
}

/// An ALO model scene.
pub struct AloScene {
    /// Uniquely identifies this instance of AloModel scene, preventing identifier collisions
    /// between different AloModels or different instantiations of the same model as independent
    /// scenes.
    instance_id: u64,
    /// Root bone of this scene, if this is a model. Absent for particle systems.
    root_bone: Option<AloBoneScene>,
}

impl AloScene {
    /// Unique type ID for the [AloModel] scene type.
    const TYPE_ID: SceneTypeId = scene_type_id!();

    /// Creates a new [AloScene] scene, allocating an instance id for it. The scene is initially
    /// empty.
    fn new() -> Self {
        static INSTANCE_ID: CallCounter = CallCounter::new();
        Self {
            instance_id: INSTANCE_ID.increment(),
            root_bone: None,
        }
    }
}

impl Scene for AloScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        self.root_bone.resolve(context, scene)?;
        Ok(())
    }

    fn register_dependencies(&self, deps: &mut SceneDependencies) {
        self.root_bone.register_dependencies(deps);
    }
}

/// Scene representing a bone from an ALO file.
struct AloBoneScene {
    /// Unique scene instance ID for the [`AloScene`] this bone is part of.
    instance_id: u64,
    /// Index of this bone within the scene, used to uniquely identify it from other bones.
    bone_idx: usize,
    /// Index of this bone's parent. Not needed for scene construction, since we use a tree
    /// structure, but it is needed to convert a flat list of bone scenes into the final tree
    /// structure.
    parent_idx: Option<usize>,
    /// Name of the bone.
    name: String,
    /// Transform to apply to this bone relative to its parent.
    transform: Transform,
    /// Inverse bindpose for this bone. This isn't used by the scene directly, but it is convenient
    /// to compute it and store it here for submeshes to use when skinning.
    inverse_bindpose: Mat4,
    /// ALO visibility setting for the bone.
    visible: bool,
    /// Billboard mode of the bone.
    billboard: BillboardMode,
    /// Child bones of this bone.
    children: Vec<AloBoneScene>,
    /// Meshes which are attached to this bone.
    meshes: Vec<AloMeshScene>,
}

impl Scene for AloBoneScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        NameEntityReference {
            name: Name(self.name.into()),
            reference: SceneEntityReference::new(
                AloScene::TYPE_ID,
                self.bone_idx,
                self.instance_id,
            ),
        }
        .resolve_inline(context, scene);
        let local_pos = self.transform.translation;
        with_template!(context, scene, AloBone);
        with_template!(context, scene, Transform, |trans| {
            *trans = self.transform;
        });
        with_template!(context, scene, AloInitialVisibility, |aiv| {
            aiv.hidden = !self.visible;
        });
        with_template!(context, scene, Visibility, |vis| {
            *vis = if self.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        });
        resolve_billboard_mode(self.billboard, local_pos, context, scene);

        let children = scene.get_or_insert_related_resolved_scenes::<ChildOf>();
        self.children.resolve_list(context, &mut children.scenes)?;
        self.meshes.resolve_list(context, &mut children.scenes)?;
        Ok(())
    }

    fn register_dependencies(&self, deps: &mut SceneDependencies) {
        self.children.register_dependencies(deps);
        self.meshes.register_dependencies(deps);
    }
}
/// Scene-like resolver that adds a bone's BillboardMode to the scene.
fn resolve_billboard_mode(
    mode: BillboardMode,
    local_pos: Vec3,
    context: &mut ResolveContext,
    scene: &mut ResolvedScene,
) {
    match mode {
        BillboardMode::Disable => {}
        BillboardMode::Face | BillboardMode::Parallel => {
            with_template!(context, scene, AloFaceCamera, |afc| {
                *afc = AloFaceCamera {
                    keep_vertical: false,
                    local_offset: local_pos,
                    rotate_around_parent: false,
                }
            })
        }
        BillboardMode::ZAxisView => {
            with_template!(context, scene, AloFaceCamera, |afc| {
                *afc = AloFaceCamera {
                    keep_vertical: true,
                    local_offset: local_pos,
                    rotate_around_parent: false,
                }
            })
        }
        BillboardMode::SunlightGlow => {
            with_template!(context, scene, AloFaceCamera, |afc| {
                *afc = AloFaceCamera {
                    keep_vertical: false,
                    local_offset: local_pos,
                    rotate_around_parent: true,
                }
            })
        }
        // TODO: support the rest of the billboard modes.
        _ => {}
    }
}

/// Scene representing a mesh in an Alo file.
struct AloMeshScene {
    /// Name of the mesh.
    name: String,
    /// Submeshes which compose this mesh.
    submeshes: Vec<AloSubmeshScene>,
    /// Hidden property from the Alo file.
    hidden: bool,
}

impl Scene for AloMeshScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        with_template!(context, scene, AloMesh);
        with_template!(context, scene, Transform);
        with_template!(context, scene, Name, |name| {
            name.set(self.name);
        });
        with_template!(context, scene, AloInitialVisibility, |aiv| {
            aiv.hidden = self.hidden;
        });
        with_template!(context, scene, Visibility, |vis| {
            *vis = if self.hidden {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        });
        let children = scene.get_or_insert_related_resolved_scenes::<ChildOf>();
        self.submeshes.resolve_list(context, &mut children.scenes)?;
        Ok(())
    }

    fn register_dependencies(&self, deps: &mut SceneDependencies) {
        self.submeshes.register_dependencies(deps);
    }
}

/// Material properties to apply to a submesh.
struct SubmeshMaterialProperties {
    /// Material to apply to this mesh.
    material: Handle<AloMaterial>,
    /// If false, this submesh should not cast shadows.
    cast_shadows: bool,
    /// Range of distances where this submesh is visible for LoD. If not set, don't use LoD.
    visibility_range: Option<VisibilityRange>,
}

/// Holds the joint mapping for a skinned submesh.
struct SubmeshJointMapping {
    /// Inverse bindposes to use if this submesh is skinned.
    inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    /// Numeric IDs of bones that this submesh uses as joints. These are the indexes within the
    /// overall skeleton, not the relative indexes within the submesh's bone mapping list, so they
    /// can be directly fed into SceneEntityReferences, since we assign scene entity references in
    /// order starting with bones.
    bones: Vec<usize>,
}

/// Scene which builds an ALO submesh.
struct AloSubmeshScene {
    /// Unique scene instance ID for the [`AloScene`] this submesh scene is a part of, used to link
    /// bone entities for joints.
    instance_id: u64,
    /// The actual mesh for this submesh.
    mesh: Handle<Mesh>,
    /// Material to apply if this mesh is visible.
    material_properties: Option<SubmeshMaterialProperties>,
    /// Joint mapping for this submesh.
    joints: Option<SubmeshJointMapping>,
}

impl Scene for AloSubmeshScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        // Add the mesh.
        with_template!(context, scene, Mesh3d, |mesh| {
            mesh.0 = self.mesh.into();
        });
        with_template!(context, scene, Transform);
        with_template!(context, scene, Visibility, |vis| {
            *vis = Visibility::Inherited;
        });

        // Add materials if set.
        if let Some(mat_props) = self.material_properties {
            with_template!(context, scene, MeshMaterial3d<AloMaterial>, |mat| {
                mat.0 = mat_props.material.into();
            });
            with_template!(context, scene, MeshTag, |tag| {
                tag.0 = 0;
            });
            if !mat_props.cast_shadows {
                with_template!(context, scene, NotShadowCaster);
            }
            // Configure LoD draw distances.
            if let Some(visibility_range) = mat_props.visibility_range {
                with_template!(context, scene, VisibilityRange, |vis_range| {
                    *vis_range = visibility_range;
                });
            }
        }

        // Link to bones if skinning.
        if let Some(joints) = self.joints {
            with_template!(context, scene, SkinnedMesh, |skinned_mesh| {
                skinned_mesh.inverse_bindposes = joints.inverse_bindposes.into();
                skinned_mesh.joints.0 = joints
                    .bones
                    .into_iter()
                    .map(|bone_idx| {
                        EntityTemplate::from_reference(
                            AloScene::TYPE_ID,
                            bone_idx,
                            self.instance_id,
                        )
                    })
                    .collect();
            });
        }

        Ok(())
    }
}

/// Mapping from a LodGroup and Lod level to a VisibilityRange to show that Lod at for that group.
type LodMappings = HashMap<(LodGroup, Lod), VisibilityRange>;

/// Loads ALO Models.
#[derive(Default, TypePath)]
struct AloLoader {
    /// Handle to the shader storage buffer containing the team colors.
    team_colors_buffer: Handle<ShaderBuffer>,
    /// Mapping from LOD levels to the range at which that LOD fades in.
    /// Each LOD starting being used when the camera is closer than this range, and stops being used
    /// in favor of a higher LOD if a higher LOD exists at whatever range the next LOD starts.
    lod_end_ranges: BTreeMap<Lod, Range<f32>>,
}

impl AloLoader {
    /// Compute level of detail ranges for the given mesh.
    fn compute_lods(&self, meshes: &[RawAloMesh]) -> Result<LodMappings, AloError> {
        let mut lod_stack = HashMap::<LodGroup, Vec<Lod>>::new();
        for mesh in meshes {
            if let Some(lod) = mesh.lod {
                lod_stack.entry(mesh.lod_group).or_default().push(lod);
            }
        }

        let mut result = LodMappings::new();
        for (group, mut lods) in lod_stack {
            lods.sort();
            for idx in 0..lods.len() {
                // If the start range for this LOD doesn't exist, we never show it.
                let Some(end_margin) = self.lod_end_ranges.get(&lods[idx]).cloned() else {
                    return Err(AloError::UnknownLod {
                        group,
                        lod: lods[idx],
                    });
                };
                // End at the start of the next LOD or 0 if this is the highest LOD.
                let start_margin = lods
                    .get(idx + 1)
                    .and_then(|lod| self.lod_end_ranges.get(lod))
                    .cloned()
                    .unwrap_or(0.0..0.0);
                let already_inserted = result
                    .insert(
                        (group, lods[idx]),
                        VisibilityRange {
                            start_margin,
                            end_margin,
                            use_aabb: false,
                        },
                    )
                    .is_some();
                if already_inserted {
                    return Err(AloError::DuplicateLod {
                        group,
                        lod: lods[idx],
                    });
                }
            }
        }
        Ok(result)
    }

    /// Converts each ALO mesh into a scene and attaches it to the correct parent bone's scene.
    fn create_mesh_scenes(
        &self,
        load_context: &mut LoadContext<'_>,
        settings: &AloMeta,
        instance_id: u64,
        meshes: Vec<RawAloMesh>,
        bones: &mut [AloBoneScene],
    ) -> Result<(), AloError> {
        let mut lod_mappings = self.compute_lods(&meshes)?;
        for (mesh_idx, mesh) in meshes.into_iter().enumerate() {
            let vis_range = match mesh.lod {
                Some(lod) => match lod_mappings.remove(&(mesh.lod_group, lod)) {
                    Some(vis_range) => Some(vis_range),
                    None => {
                        return Err(AloError::UnknownLod {
                            group: mesh.lod_group,
                            lod,
                        });
                    }
                },
                // If the mesh has no LOD setting, don't use a visibility range.
                None => None,
            };
            let name = make_ascii_uppercase(mesh.name);

            let submeshes = self.create_submesh_scenes(
                load_context,
                settings,
                instance_id,
                mesh_idx,
                vis_range.as_ref(),
                SpecialNameCategory::categorize(&name),
                mesh.submeshes,
                bones,
            )?;

            let bone = match bones.get_mut(mesh.parent_bone) {
                Some(bone) => bone,
                None => {
                    return Err(AloError::InvalidParentIndex {
                        category: "Mesh",
                        item_idx: mesh_idx,
                        parent_idx: mesh.parent_bone,
                    });
                }
            };
            bone.meshes.push(AloMeshScene {
                hidden: mesh.hidden,
                name,
                submeshes,
            });
        }

        Ok(())
    }

    /// Converts eash ALO submesh into a scene.
    fn create_submesh_scenes(
        &self,
        load_context: &mut LoadContext<'_>,
        settings: &AloMeta,
        instance_id: u64,
        mesh_idx: usize,
        vis_range: Option<&VisibilityRange>,
        category: SpecialNameCategory,
        submeshes: Vec<Submesh>,
        bones: &[AloBoneScene],
    ) -> Result<Vec<AloSubmeshScene>, AloError> {
        let mut res = Vec::with_capacity(submeshes.len());
        for (submesh_idx, submesh) in submeshes.into_iter().enumerate() {
            let mesh = build_submesh_mesh(
                submesh.vertex_format,
                submesh.primitives,
                submesh.vertex_data,
            );
            let mesh = load_context.add_labeled_asset(
                AloAssetLabel::Submesh {
                    mesh_idx,
                    submesh_idx,
                }
                .to_string(),
                mesh,
            );

            let material = build_submesh_material(
                load_context,
                settings,
                submesh.shader,
                self.team_colors_buffer.clone(),
            )?;
            let material = material.map(|material| {
                load_context.add_labeled_asset(
                    AloAssetLabel::SubmeshMaterial {
                        mesh_idx,
                        submesh_idx,
                    }
                    .to_string(),
                    material,
                )
            });
            let material_properties = material.map(|material| SubmeshMaterialProperties {
                material,
                cast_shadows: category != SpecialNameCategory::Blast,
                visibility_range: vis_range.cloned(),
            });

            let joints = if submesh.vertex_format.contains(VertexFormat::Bones) {
                let mut inverse_bindposes = Vec::with_capacity(submesh.bone_mappings.len());
                for &mapping in &submesh.bone_mappings {
                    // We trust that this is in range because petro-alo validates it for us.
                    inverse_bindposes.push(bones[mapping].inverse_bindpose);
                }
                let inverse_bindposes = load_context.add_labeled_asset(
                    AloAssetLabel::SubmeshInverseBindposes {
                        mesh_idx,
                        submesh_idx,
                    }
                    .to_string(),
                    SkinnedMeshInverseBindposes::from(inverse_bindposes),
                );

                Some(SubmeshJointMapping {
                    inverse_bindposes,
                    // This is already checked to be in-range by petro-alo library.
                    bones: submesh.bone_mappings,
                })
            } else {
                None
            };

            res.push(AloSubmeshScene {
                instance_id,
                mesh,
                material_properties,
                joints,
            });
        }
        Ok(res)
    }
}

/// Metadata settings for an individual ALO model load.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AloMeta {
    /// Mapping from texture names as they appear in the model file to asset paths to use for those
    /// textures. Any texture not mapped here will try to use the original path from the model file.
    pub texture_mapping: BTreeMap<PathBuf, AssetPath<'static>>,
}

impl AloMeta {
    /// Create a new empty [`AloMeta`] with no mappings.
    pub fn new() -> Self {
        Default::default()
    }
}

impl AssetLoader for AloLoader {
    type Asset = ScenePatch;
    type Error = AloError;
    type Settings = AloMeta;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let file = AloFile::parse(&bytes)?;
        let mut scene = AloScene::new();

        let mut bones = bones_to_scenes(scene.instance_id, file.skeleton);
        self.create_mesh_scenes(
            load_context,
            settings,
            scene.instance_id,
            file.meshes,
            &mut bones,
        )?;

        scene.root_bone = flat_bones_to_tree(bones)?;

        Ok(ScenePatch::load_with(load_context, scene))
    }

    fn extensions(&self) -> &[&str] {
        &["alo"]
    }
}

/// Converts the skeleton's list of bones into a flat list of empty bone scenes in the same order.
/// Does not build the parenting tree.
///
/// * `instance_id`: Unique instance id of the [AloScene] this bone is part of.
/// * `skeleton`: the [Skeleton] to build into a scene.
fn bones_to_scenes(instance_id: u64, skeleton: Skeleton) -> Vec<AloBoneScene> {
    let bevy_to_alo = ALO_TO_BEVY.inverse();
    // Iterate in reverse so we can insert each bone into the child collector for the previous.
    skeleton
        .bones
        .into_iter()
        .enumerate()
        .map(|(bone_idx, bone)| {
            let name = make_ascii_uppercase(bone.name);
            // To convert a matrix from alo to bevy, we create a matrix which first transforms
            // from bevy space to alo space, then performs the transform in alo space, then
            // transforms back to bevy space.
            let transform =
                Transform::from_matrix(ALO_TO_BEVY * bone.local_to_parent * bevy_to_alo);
            let inverse_bindpose = (ALO_TO_BEVY * bone.local_to_global * bevy_to_alo).inverse();
            AloBoneScene {
                instance_id,
                bone_idx,
                parent_idx: bone.parent,
                name,
                transform,
                inverse_bindpose,
                visible: bone.visible,
                billboard: bone.billboard,
                // Children of this bone will be filled in later.
                children: Vec::new(),
                meshes: Vec::new(),
            }
        })
        .collect()
}

/// Converts a vector of ALO bones into a tree of bone scenes.
///
/// This assumes the bones in the list have the same properties as are expected in an ALO file:
/// -   The root bone is first
/// -   All bones come after their parents (all parent references point to lower indexes in the
///     vector).
/// -   All bones besides the root have a parent.
fn flat_bones_to_tree(mut bones: Vec<AloBoneScene>) -> Result<Option<AloBoneScene>, AloError> {
    while let Some(bone) = bones.pop() {
        debug_assert!(bone.bone_idx == bones.len());
        match bone.parent_idx {
            Some(parent_idx) => match bones.get_mut(parent_idx) {
                Some(parent) => parent.children.push(bone),
                None => {
                    return Err(AloError::InvalidParentIndex {
                        category: "Bone",
                        item_idx: bone.bone_idx,
                        parent_idx,
                    });
                }
            },
            // This bone is the root, we are done collecting.
            None if bone.bone_idx == 0 => return Ok(Some(bone)),
            None => {
                return Err(AloError::MissingParent {
                    bone_idx: bone.bone_idx,
                });
            }
        }
    }
    // If the list was initially empty, then there is no root bone.
    Ok(None)
}

/// Build the mesh for a submesh.
fn build_submesh_mesh(
    vertex_format: VertexFormat,
    index_buffer: Vec<[u16; 3]>,
    vertex_data: SubmeshVertexData,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        // We have to retain all meshes on the CPU side because we dn't know which will
        // be needed for features like collision detection, as that is configured later.
        RenderAssetUsages::all(),
    );
    mesh.insert_indices(Indices::U16(cast_vec(index_buffer)));
    if vertex_format.contains(VertexFormat::Positions) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, alo_to_bevy(vertex_data.positions));
    }
    if vertex_format.intersects(VertexFormat::Normals | VertexFormat::TangentBinormals) {
        let normals = alo_to_bevy(vertex_data.normals);
        if vertex_format.contains(VertexFormat::TangentBinormals) {
            let tangents = combine_tangents(
                &normals,
                &alo_to_bevy(vertex_data.tangents),
                &alo_to_bevy(vertex_data.binormals),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
        }
        if vertex_format.contains(VertexFormat::Normals) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
    }
    if vertex_format.contains(VertexFormat::TexCoords) {
        let [uv0, uv1, _, _] = vertex_data.tex_coords;
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
    }
    if vertex_format.contains(VertexFormat::Colors) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vertex_data.colors);
    }
    if vertex_format.contains(VertexFormat::Bones) {
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vertex_data.bone_indices),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vertex_data.bone_weights);
    }
    mesh
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
                    value_type: shader_param_type_name(&other),
                    expected: stringify!($ty),
                })
            }
            None => None,
        }
    };
}

/// Gets the name of a shader param type.
fn shader_param_type_name(param: &ShaderParam) -> &'static str {
    match param {
        ShaderParam::Int(_) => "Int",
        ShaderParam::Float(_) => "Float",
        ShaderParam::Float3(_) => "Float3",
        ShaderParam::Texture(_) => "Texture",
        ShaderParam::Float4(_) => "Float4",
    }
}

/// Convert an ALO shader to a Bevy material (within reason).
///
/// This will destructively take the shader parameters leaving the parameter map empty.
fn build_submesh_material(
    load_context: &mut LoadContext<'_>,
    settings: &AloMeta,
    mut shader: AloShader,
    team_colors: Handle<ShaderBuffer>,
) -> Result<Option<AloMaterial>, AloError> {
    let mut base = StandardMaterial::default();
    let mut extension = AloMaterialExtension::new(team_colors);
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
        let tex = load_alo_texture(load_context, settings, tex, true);
        base.base_color_texture = Some(tex);
    }
    if let Some(tex) = expect_shader_param!(shader, "NormalTexture", Texture) {
        let tex = load_alo_texture(load_context, settings, tex, false);
        base.normal_map_texture = Some(tex);
    }
    if let Some(tex) = expect_shader_param!(shader, "GlossTexture", Texture) {
        let tex = load_alo_texture(load_context, settings, tex, false);
        extension.gloss_texture = Some(tex);
    }

    Ok(Some(ExtendedMaterial { base, extension }))
}

fn load_alo_texture<'a>(
    load_context: &mut LoadContext<'_>,
    settings: &AloMeta,
    path: PathBuf,
    is_srgb: bool,
) -> Handle<Image> {
    let asset_path = match settings.texture_mapping.get(&path) {
        Some(asset_path) => asset_path.clone(),
        None => AssetPath::from(path),
    };
    load_context
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = is_srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(asset_path)
}

/// Indicates if the mesh name contains text that indicates it should be treated specially.
#[derive(Copy, Clone, PartialEq, Eq)]
enum SpecialNameCategory {
    /// Don't apply any special handling based on the mesh's name.
    None,
    /// The name contains BLAST or other indicator that it is a damage decal. These meshes should
    /// not cast shadows.
    Blast,
}

impl SpecialNameCategory {
    /// Categorize a mesh name. The name must already have been converted to upper case.
    fn categorize(name: &str) -> Self {
        match name {
            _ if name.ends_with("BLAST") => Self::Blast,
            _ => Self::None,
        }
    }
}

/// Converts a name string to ascii uppercase. Bones and meshes are often named in mixed case but
/// referenced in uppercase, so we upcase them for consistency.
fn make_ascii_uppercase(mut name: String) -> String {
    name.make_ascii_uppercase();
    name
}
