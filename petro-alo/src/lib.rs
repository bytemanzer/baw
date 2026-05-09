use anyhow::Context;
use petro_chunked::iter_chunks;
use thiserror::Error;

pub use crate::error::AloError;
use crate::error::FormatError;
use crate::mesh::Mesh;
use crate::skeleton::Skeleton;

mod error;
mod ids;
pub mod mesh;
mod primitives;
pub mod skeleton;

#[derive(Debug)]
pub struct AloFile {
    pub skeleton: Skeleton,
    pub meshes: Vec<Mesh>,
}

impl AloFile {
    pub fn parse(bytes: &[u8]) -> Result<AloFile, AloError> {
        let mut chunks = iter_chunks(bytes);

        let mut skeleton = None;
        let mut meshes = Vec::new();

        loop {
            let chunk = chunks
                .try_expect_chunk([
                    ids::SKELETON,
                    ids::MESH,
                    ids::LIGHT,
                    ids::CONNECTIONS,
                    ids::PARTICLES,
                ])
                .context("getting a chunk from the top level of the ALO file")?;
            let chunk = match chunk {
                Some(chunk) => chunk,
                None => break,
            };

            match chunk.id() {
                ids::SKELETON => {
                    if skeleton.is_some() {
                        Err(FormatError::DuplicateChunk { id: chunk.id() })
                            .context("already found a skeleton chunk")?;
                    }
                    skeleton = Some(Skeleton::parse(chunk.content())?);
                }
                ids::MESH => {
                    #[derive(Error, Debug)]
                    #[error("parsing mesh {idx}")]
                    struct MeshCtx {
                        idx: usize,
                    }
                    let mesh =
                        Mesh::parse(chunk.content()).context(MeshCtx { idx: meshes.len() })?;
                    meshes.push(mesh);
                }
                _ => {}
            }
        }

        #[derive(Error, Debug, Copy, Clone)]
        #[error("validating submesh {smidx} of mesh {midx}")]
        struct SubmeshValidationCtx {
            midx: usize,
            smidx: usize,
        }
        for (midx, mesh) in meshes.iter().enumerate() {
            for (smidx, submesh) in mesh.submeshes.iter().enumerate() {
                let ctx = SubmeshValidationCtx { midx, smidx };
                if !submesh.bone_mappings.is_empty() {
                    let Some(ref skeleton) = skeleton else {
                        return Err(anyhow::Error::new(FormatError::MissingSkeleton)
                            .context(ctx)
                            .into());
                    };
                    for &bone_index in &submesh.bone_mappings {
                        if bone_index as u64 >= skeleton.bones.len() as u64 {
                            Err(FormatError::MappingIndexOutOfRange {
                                idx: bone_index,
                                count: skeleton.bones.len(),
                            })
                            .context(ctx)?;
                        }
                    }
                }
            }
        }

        chunks
            .expect_empty()
            .context("leftover chunks at the top level of the ALO file")?;

        Ok(AloFile {
            skeleton: skeleton.unwrap_or_default(),
            meshes,
        })
    }
}
