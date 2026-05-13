use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Context;
use bitflags::bitflags;
use glam::Vec3;
use petro_chunked::ChunkContent;
use petro_chunked::byteblock::{ConvertBytes, TakeBytes};
use thiserror::Error;

use crate::error::FormatError;
use crate::ids;
use crate::primitives::{U32Bool, U32Usize, U32UsizeVec};

/// Bounding box for a mesh.
#[derive(Default, Debug, Copy, Clone)]
pub struct BoundingBox {
    pub start: Vec3,
    pub end: Vec3,
}

/// A mesh in an ALO model.
#[derive(Default, Debug)]
pub struct Mesh {
    /// Name of this mesh.
    pub name: String,
    /// Bounding box of the mesh.
    pub bounds: BoundingBox,
    /// Whether the mesh is (initially) hidden.
    pub hidden: bool,
    /// Whether this mesh is used for collision.
    pub collision_enabled: bool,
    pub submeshes: Vec<Submesh>,
    /// Bone to attach this mesh to.
    pub parent_bone: Option<usize>,
}

impl Mesh {
    pub(crate) fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut chunks = content.children().context("parsing mesh")?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the mesh name (chunk 00000401)")]
        struct MeshNameCtx;

        let name = chunks
            .expect_chunk(ids::MESH_NAME)
            .context(MeshNameCtx)?
            .data()
            .context(MeshNameCtx)?
            .convert()
            .context(MeshNameCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the mesh info (chunk 00000402)")]
        struct MeshInfoCtx;

        type MeshInfo<'b> = (U32Usize, [[f32; 3]; 2], u32, U32Bool, U32Bool, &'b [u8; 88]);
        let (
            U32Usize {
                value: submesh_count,
            },
            bounds,
            _unused,
            U32Bool { value: hidden },
            U32Bool {
                value: collision_enabled,
            },
            _padding,
        ): MeshInfo = chunks
            .expect_chunk(ids::MESH_INFO)
            .context(MeshInfoCtx)?
            .data()
            .context(MeshInfoCtx)?
            .convert()
            .context(MeshInfoCtx)?;
        let bounds = BoundingBox {
            start: Vec3::from_array(bounds[0]),
            end: Vec3::from_array(bounds[1]),
        };

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing submesh {submesh_idx} / {submesh_count}")]
        struct SubmeshCtx {
            submesh_count: usize,
            submesh_idx: usize,
        }

        let mut submeshes = Vec::new();
        for submesh_idx in 0..submesh_count {
            let ctx = SubmeshCtx {
                submesh_count,
                submesh_idx,
            };
            let mat = chunks.expect_chunk(ids::SUBMESH_MATERIAL).context(ctx)?;
            let data = chunks.expect_chunk(ids::SUBMESH_DATA).context(ctx)?;
            let submesh = Submesh::parse(mat.content(), data.content()).context(ctx)?;
            submeshes.push(submesh);
        }

        chunks.expect_empty().context("after parsing submeshes")?;

        Ok(Mesh {
            name,
            bounds,
            hidden,
            collision_enabled,
            submeshes,
            parent_bone: None,
        })
    }
}

bitflags! {
    /// Identifies which vertex fields should actually be bound.
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct VertexFormat : u32 {
        const Positions = 0x01;
        const Normals = 0x02;
        const TexCoords = 0x04;
        const TangentBinormals = 0x08;
        const Colors = 0x10;
        const Bones = 0x20;
    }
}

impl FromStr for VertexFormat {
    type Err = FormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let vertex_format = match s {
            "alD3dVertN" => VertexFormat::Positions | VertexFormat::Normals,
            "alD3dVertNU2" => {
                VertexFormat::Positions | VertexFormat::Normals | VertexFormat::TexCoords
            }
            "alD3dVertNU2C" => {
                VertexFormat::Positions
                    | VertexFormat::Normals
                    | VertexFormat::TexCoords
                    | VertexFormat::Colors
            }
            "alD3dVertNU2U3U3" => {
                VertexFormat::Positions
                    | VertexFormat::Normals
                    | VertexFormat::TexCoords
                    | VertexFormat::TangentBinormals
            }
            "alD3dVertNU2U3U3C" => {
                VertexFormat::Positions
                    | VertexFormat::Normals
                    | VertexFormat::TexCoords
                    | VertexFormat::TangentBinormals
                    | VertexFormat::Colors
            }
            "alD3dVertRSkinNU2" | "alD3dVertB4I4NU2" => {
                VertexFormat::Positions
                    | VertexFormat::Normals
                    | VertexFormat::TexCoords
                    | VertexFormat::Bones
            }
            "alD3dVertRSkinNU2U3U3" | "alD3dVertB4I4NU2U3U3" => {
                VertexFormat::Positions
                    | VertexFormat::Normals
                    | VertexFormat::TexCoords
                    | VertexFormat::TangentBinormals
                    | VertexFormat::Bones
            }
            "alD3dVertGrass" => {
                // TODO: ????? what should this do
                VertexFormat::Positions
            }
            _ => {
                return Err(FormatError::UnknownVertexFormat {
                    format: s.to_owned(),
                });
            }
        };
        Ok(vertex_format)
    }
}

/// Holds properties of a SubMesh.
#[derive(Debug)]
pub struct Submesh {
    /// Shader to use for this mesh.
    pub shader: Shader,
    /// Vertex format of this submesh. Determines which of the vertex buffers should actually be
    /// used.
    ///
    /// TODO: Figure out what each of the values actually means and avoid copying (leave empty) the
    /// vertex buffers which aren't used.
    pub vertex_format_string: String,
    /// Interpreted vertex format.
    pub vertex_format: VertexFormat,
    /// Buffer of vertex indices. Each index points into all of the vertex buffers, sets of 3
    /// vertices make a triangle.
    pub primitives: Vec<[u16; 3]>,
    /// Mappings from indexes used by bone_indices (which refer to this array) to bones in the
    /// skeleton segment (referenced by the u32 values held here).
    pub bone_mappings: Vec<usize>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tex_coords: [Vec<[f32; 2]>; 4],
    pub tangents: Vec<[f32; 3]>,
    pub binormals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    /// Indexes into the bone_mappings array.
    pub bone_indices: Vec<[u16; 4]>,
    pub bone_weights: Vec<[f32; 4]>,
}

impl Submesh {
    /// Parse a Submesh from the materials and data chunks.
    fn parse(materials: ChunkContent, data: ChunkContent) -> anyhow::Result<Self> {
        let shader =
            Shader::parse(materials).context("parsing the submesh material information")?;

        let mut data = data.children()?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the submesh info (chunk 00010001)")]
        struct SubmeshInfoCtx;
        let (
            U32Usize {
                value: num_vertices,
            },
            U32Usize {
                value: num_primitives,
            },
            _padding,
        ): (_, _, &[u8; 120]) = data
            .expect_chunk(ids::SUBMESH_INFO)
            .context(SubmeshInfoCtx)?
            .data()
            .context(SubmeshInfoCtx)?
            .convert()
            .context(SubmeshInfoCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the submesh vertex format (chunk 00010002)")]
        struct VertexFmtCtx;
        let vertex_format_string: String = data
            .expect_chunk(ids::SUBMESH_VERTEX_FORMAT)
            .context(VertexFmtCtx)?
            .data()
            .context(VertexFmtCtx)?
            .convert()
            .context(VertexFmtCtx)?;
        let vertex_format = vertex_format_string.parse().context(VertexFmtCtx)?;

        let mut submesh = Submesh {
            shader,
            vertex_format_string,
            vertex_format,
            primitives: Vec::new(),
            bone_mappings: Vec::new(),
            positions: Vec::with_capacity(num_vertices),
            normals: Vec::with_capacity(num_vertices),
            tex_coords: [
                Vec::with_capacity(num_vertices),
                Vec::with_capacity(num_vertices),
                Vec::with_capacity(num_vertices),
                Vec::with_capacity(num_vertices),
            ],
            tangents: Vec::with_capacity(num_vertices),
            binormals: Vec::with_capacity(num_vertices),
            colors: Vec::with_capacity(num_vertices),
            bone_indices: Vec::with_capacity(num_vertices),
            bone_weights: Vec::with_capacity(num_vertices),
        };

        loop {
            let chunk = data.try_expect_chunk([
                ids::SUBMESH_INDEX_BUF,
                ids::SUBMESH_ANIM_MAP,
                ids::SUBMESH_VERTEX_BUF_V1,
                ids::SUBMESH_VERTEX_BUF_V2,
                ids::SUBMESH_COLLISION,
            ])?;
            let chunk = match chunk {
                Some(chunk) => chunk,
                None => break,
            };
            match chunk.id() {
                ids::SUBMESH_INDEX_BUF => {
                    submesh
                        .parse_index_buf(chunk.content(), num_primitives)
                        .context("parsing the submesh index buffer (chunk 00010004)")?;
                }
                ids::SUBMESH_ANIM_MAP => {
                    submesh
                        .parse_bone_mapping(chunk.content())
                        .context("parsing the submesh animation mapping (chunk 00010006)")?;
                }
                ids::SUBMESH_VERTEX_BUF_V1 => {
                    submesh
                        .parse_v1_vertex_buf(chunk.content(), num_vertices)
                        .context("parsing the submesh v1 vertex buffer (chunk 00010005)")?;
                }
                ids::SUBMESH_VERTEX_BUF_V2 => {
                    submesh
                        .parse_v2_vertex_buf(chunk.content(), num_vertices)
                        .context("parsing the submesh v1 vertex buffer (chunk 00010007)")?;
                }
                ids::SUBMESH_COLLISION => {}
                _ => unreachable!(),
            }
        }

        data.expect_empty()?;

        if submesh.primitives.is_empty() {
            Err(FormatError::MissingPrimitives)?;
        }
        if submesh.positions.is_empty() {
            Err(FormatError::MissingVertices)?;
        }
        for prim in &submesh.primitives {
            for &idx in prim {
                if idx as usize >= submesh.positions.len() {
                    Err(FormatError::PrimitiveIndexOutOfRange {
                        idx,
                        len: submesh.positions.len(),
                    })?;
                }
            }
        }
        if !submesh.bone_mappings.is_empty() {
            let bone_index_count = submesh.bone_mappings.len().min(u16::MAX as usize) as u16;
            for (bidx, &bwt) in submesh
                .bone_indices
                .iter_mut()
                .zip(submesh.bone_weights.iter())
            {
                *bidx = squash_bone_indices(*bidx, bwt, bone_index_count)?;
            }
        }

        Ok(submesh)
    }

    fn parse_index_buf(&mut self, data: ChunkContent, expected: usize) -> anyhow::Result<()> {
        if !self.primitives.is_empty() {
            Err(FormatError::DuplicateChunk {
                id: ids::SUBMESH_INDEX_BUF,
            })?;
        }
        self.primitives = data.data()?.convert()?;

        if self.primitives.len() != expected {
            Err(FormatError::WrongContentCount {
                expected,
                actual: self.primitives.len(),
            })?;
        }
        Ok(())
    }

    fn parse_v1_vertex_buf(&mut self, data: ChunkContent, expected: usize) -> anyhow::Result<()> {
        type VertexV1 = (
            // Position
            [f32; 3],
            // Normal
            [f32; 3],
            // Tex coords
            [[f32; 2]; 4],
            // Tangent
            [f32; 3],
            // Binormal
            [f32; 3],
            // Color
            [f32; 4],
            // Bone indices
            [u32; 4],
            // Bone weights
            [f32; 4],
        );

        if !self.positions.is_empty() {
            Err(FormatError::DuplicateChunk {
                id: ids::SUBMESH_VERTEX_BUF_V1,
            })?;
        }

        let mut vert_data = data.data()?.as_data();
        while !vert_data.is_empty() {
            let (pos, norm, [t0, t1, t2, t3], tan, binorm, col, bidx, bwt): VertexV1 =
                vert_data.take()?;
            let bidx = squash_bone_indices(bidx, bwt, u16::MAX)?;
            self.positions.push(pos);
            self.normals.push(norm);
            self.tex_coords[0].push(t0);
            self.tex_coords[1].push(t1);
            self.tex_coords[2].push(t2);
            self.tex_coords[3].push(t3);
            self.tangents.push(tan);
            self.binormals.push(binorm);
            self.colors.push(col);
            self.bone_indices.push(bidx);
            self.bone_weights.push(bwt);
        }

        if self.positions.len() != expected {
            Err(FormatError::WrongContentCount {
                expected,
                actual: self.positions.len(),
            })?;
        }
        Ok(())
    }

    fn parse_v2_vertex_buf(&mut self, data: ChunkContent, expected: usize) -> anyhow::Result<()> {
        type VertexV2 = (
            // Position
            [f32; 3],
            // Normal
            [f32; 3],
            // Tex coords
            [[f32; 2]; 4],
            // Tangent
            [f32; 3],
            // Binormal
            [f32; 3],
            // Color
            [f32; 4],
            // Unused.
            [f32; 4],
            // Bone indices
            [u32; 4],
            // Bone weights
            [f32; 4],
        );

        if !self.positions.is_empty() {
            Err(FormatError::DuplicateChunk {
                id: ids::SUBMESH_VERTEX_BUF_V2,
            })?;
        }

        let mut vert_data = data.data()?.as_data();
        while !vert_data.is_empty() {
            let (pos, norm, [t0, t1, t2, t3], tan, binorm, col, _unused, bidx, bwt): VertexV2 =
                vert_data.take()?;
            let bidx = squash_bone_indices(bidx, bwt, u16::MAX)?;
            self.positions.push(pos);
            self.normals.push(norm);
            self.tex_coords[0].push(t0);
            self.tex_coords[1].push(t1);
            self.tex_coords[2].push(t2);
            self.tex_coords[3].push(t3);
            self.tangents.push(tan);
            self.binormals.push(binorm);
            self.colors.push(col);
            self.bone_indices.push(bidx);
            self.bone_weights.push(bwt);
        }

        if self.positions.len() != expected {
            Err(FormatError::WrongContentCount {
                expected,
                actual: self.positions.len(),
            })?;
        }
        Ok(())
    }

    fn parse_bone_mapping(&mut self, data: ChunkContent) -> anyhow::Result<()> {
        if !self.bone_mappings.is_empty() {
            Err(FormatError::DuplicateChunk {
                id: ids::SUBMESH_ANIM_MAP,
            })?;
        }
        let U32UsizeVec { value } = data.data()?.convert()?;
        self.bone_mappings = value;
        Ok(())
    }
}

/// Squash an array of u32 values down to u16.
fn squash_bone_indices(
    [i1, i2, i3, i4]: [impl Into<u32>; 4],
    [w1, w2, w3, w4]: [f32; 4],
    index_count: u16,
) -> anyhow::Result<[u16; 4]> {
    Ok([
        squash_bone_idx(i1, w1, index_count)?,
        squash_bone_idx(i2, w2, index_count)?,
        squash_bone_idx(i3, w3, index_count)?,
        squash_bone_idx(i4, w4, index_count)?,
    ])
}

fn squash_bone_idx(idx: impl Into<u32>, weight: f32, index_count: u16) -> anyhow::Result<u16> {
    let idx = idx.into();
    if idx >= index_count as u32 {
        // Some models have index 3 set to a large number for unknown reason.
        if weight == 0.0 {
            Ok(0)
        } else {
            Err(FormatError::BoneNotMapped { idx }.into())
        }
    } else {
        Ok(idx as u16)
    }
}

/// Submesh shader configuration.
#[derive(Debug, Clone)]
pub struct Shader {
    /// Name of the shader file.
    pub filename: String,
    /// Shader parameters for this submesh's shader.
    pub params: HashMap<String, ShaderParam>,
}

impl Shader {
    fn parse(materials: ChunkContent) -> anyhow::Result<Self> {
        let mut materials = materials.children()?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the submesh material shader name (chunk 00010101)")]
        struct ShaderNameCtx;
        let filename: String = materials
            .expect_chunk(ids::SUBMESH_MATERIAL_SHADER_NAME)
            .context(ShaderNameCtx)?
            .data()
            .context(ShaderNameCtx)?
            .convert()
            .context(ShaderNameCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the submesh material parameter in chunk {id:0>8x}")]
        struct ShaderParamCtx {
            id: u32,
        }

        let mut params = HashMap::new();
        while let Some(chunk) = materials
            .try_expect_chunk([
                ids::SUBMESH_MATERIAL_INT_PARAM,
                ids::SUBMESH_MATERIAL_FLOAT_PARAM,
                ids::SUBMESH_MATERIAL_FLOAT3_PARAM,
                ids::SUBMESH_MATERIAL_TEX_PARAM,
                ids::SUBMESH_MATERIAL_FLOAT4_PARAM,
            ])
            .context("reading shader params")?
        {
            let ctx = ShaderParamCtx { id: chunk.id() };
            let mut content = chunk.data().context(ctx)?.as_mini_chunks();
            let name = content
                .expect_chunk(ids::SUBMESH_MATERIAL_PARAM_NAME)
                .context(ctx)?
                .convert()
                .context(ctx)?;
            if params.contains_key(&name) {
                return Err(FormatError::DuplicateShaderParam { name }).context(ctx)?;
            }
            let value = content
                .expect_chunk(ids::SUBMESH_MATERIAL_PARAM_VALUE)
                .context(ctx)?;
            let value = match chunk.id() {
                ids::SUBMESH_MATERIAL_INT_PARAM => ShaderParam::Int(value.convert().context(ctx)?),
                ids::SUBMESH_MATERIAL_FLOAT_PARAM => {
                    ShaderParam::Float(value.convert().context(ctx)?)
                }
                ids::SUBMESH_MATERIAL_FLOAT3_PARAM => {
                    ShaderParam::Float3(value.convert().context(ctx)?)
                }
                ids::SUBMESH_MATERIAL_TEX_PARAM => {
                    ShaderParam::Texture(value.convert().context(ctx)?)
                }
                ids::SUBMESH_MATERIAL_FLOAT4_PARAM => {
                    ShaderParam::Float4(value.convert().context(ctx)?)
                }
                _ => unreachable!(),
            };
            params.insert(name, value);
        }
        materials
            .expect_empty()
            .context("after reading shader parameters")?;

        Ok(Self { filename, params })
    }
}

/// A value passed to an ALO shader.
#[derive(Debug, Clone)]
pub enum ShaderParam {
    Int(i32),
    Float(f32),
    Float3([f32; 3]),
    Texture(String),
    Float4([f32; 4]),
}
