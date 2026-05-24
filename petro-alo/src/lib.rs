use std::collections::HashMap;

use anyhow::Context;
use petro_chunked::iter_chunks;
use thiserror::Error;

use crate::alternatives::{Alt, Lod, LodGroup};
use crate::connections::Connections;
pub use crate::error::AloError;
use crate::error::FormatError;
use crate::mesh::Mesh;
use crate::skeleton::{Proxy, Skeleton};

pub mod alternatives;
mod connections;
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

        #[derive(Copy, Clone)]
        enum ConnectionIdx {
            Mesh(usize),
            Light(usize),
        }

        let mut skeleton = None;
        let mut meshes = Vec::new();
        let mut lights = 0;
        let mut connection_indices = Vec::new();
        let mut connections = None;

        loop {
            let chunk = chunks
                .try_expect_chunk([
                    ids::SKELETON,
                    ids::MESH,
                    ids::LIGHT,
                    ids::CONNECTION,
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
                    connection_indices.push(ConnectionIdx::Mesh(meshes.len()));
                    meshes.push(mesh);
                }
                ids::LIGHT => {
                    connection_indices.push(ConnectionIdx::Light(lights));
                    lights += 1;
                }
                ids::CONNECTION => {
                    if connections.is_some() {
                        Err(FormatError::DuplicateChunk { id: chunk.id() })
                            .context("already found a connection chunk")?;
                    }
                    connections = Some(Connections::parse(chunk.content())?);
                }
                _ => {}
            }
        }

        chunks
            .expect_empty()
            .context("leftover chunks at the top level of the ALO file")?;

        let mut skeleton = skeleton.unwrap_or_default();

        #[derive(Error, Debug, Copy, Clone)]
        #[error("validating submesh {smidx} of mesh {midx}")]
        struct SubmeshValidationCtx {
            midx: usize,
            smidx: usize,
        }

        let mut lod_groups = HashMap::<&str, LodGroup>::new();
        for (midx, mesh) in meshes.iter_mut().enumerate() {
            if mesh.lod.is_some() {
                let lod_group_name = Lod::lod_group_name(&mesh.name);
                let next_lod_group_id = lod_groups.len();
                mesh.lod_group = *lod_groups
                    .entry(lod_group_name)
                    .or_insert(LodGroup(next_lod_group_id));
            }
            for (smidx, submesh) in mesh.submeshes.iter().enumerate() {
                let ctx = SubmeshValidationCtx { midx, smidx };
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

        if let Some(connections) = connections {
            #[derive(Error, Debug, Copy, Clone)]
            #[error("wiring object connection {obj_idx}")]
            struct ObjConnCtx {
                obj_idx: usize,
            }
            for (obj_idx, obj_conn) in connections.objects.into_iter().enumerate() {
                let ctx = ObjConnCtx { obj_idx };
                if obj_conn.bone >= skeleton.bones.len() {
                    Err(FormatError::ConnBoneOutOfRange {
                        idx: obj_conn.bone,
                        count: skeleton.bones.len(),
                    })
                    .context(ctx)?;
                }

                let obj = connection_indices
                    .get(obj_conn.object)
                    .ok_or(FormatError::ConnObjectOutOfRange {
                        idx: obj_conn.object,
                        count: connection_indices.len(),
                    })
                    .context(ctx)?;
                match *obj {
                    ConnectionIdx::Mesh(mesh_idx) => {
                        let mesh = &mut meshes[mesh_idx];
                        if mesh.parent_bone.is_some() {
                            Err(FormatError::MultiParentMesh { mesh_idx }).context(ctx)?;
                        }
                        mesh.parent_bone = Some(obj_conn.bone);
                    }
                    ConnectionIdx::Light(_) => {}
                }
            }
            #[derive(Error, Debug, Copy, Clone)]
            #[error("wiring proxy connection {proxy_idx}")]
            struct ProxyConnCtx {
                proxy_idx: usize,
            }
            for (proxy_idx, proxy_conn) in connections.proxies.into_iter().enumerate() {
                let ctx = ProxyConnCtx { proxy_idx };
                let num_bones = skeleton.bones.len();
                let bone = skeleton
                    .bones
                    .get_mut(proxy_conn.bone)
                    .ok_or(FormatError::ConnObjectOutOfRange {
                        idx: proxy_conn.bone,
                        count: num_bones,
                    })
                    .context(ctx)?;
                bone.proxies.push(Proxy {
                    alt: Alt::from_name(&proxy_conn.proxy_name),
                    name: proxy_conn.proxy_name,
                    hidden: proxy_conn.hidden,
                    alt_decrease_stay_hidden: proxy_conn.alt_decrease_stay_hidden,
                });
            }
        }

        Ok(AloFile { skeleton, meshes })
    }
}
