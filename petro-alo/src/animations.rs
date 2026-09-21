// Temporary because this module is incomplete.
#![allow(unused)]

use anyhow::Context;
use glam::Vec3;
use petro_chunked::chunks::ChunksIter;
use petro_chunked::minichunk::mini_chunk_map;
use thiserror::Error;

use crate::primitives::U32Usize;
use crate::{AlaFile, ids};

impl AlaFile {
    pub(crate) fn parse_v1(
        mut chunks: ChunksIter,
        num_frames: usize,
        fps: f32,
        num_bones: usize,
    ) -> Result<Self, anyhow::Error> {
        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the bone data (chunk 0x00001002) for bone index {bidx}")]
        struct BoneAnimCtx {
            bidx: usize,
        }

        let mut bone_animations = Vec::with_capacity(num_bones);
        for bidx in 0..num_bones {
            let ctx = BoneAnimCtx { bidx };
            let bone_chunks = chunks
                .expect_chunk(ids::ANIMATION_BONE_DATA)
                .context(ctx)?
                .children()
                .context(ctx)?;
            let animation = BoneAnimation::parse_v1(bone_chunks, num_frames)?;
            bone_animations.push(animation);
        }

        Ok(AlaFile {
            fps,
            bone_animations,
        })
    }

    pub(crate) fn parse_v2(
        chunks: ChunksIter,
        num_frames: usize,
        fps: f32,
        num_bones: usize,
        rot_frame_size: usize,
        tra_frame_size: usize,
        sca_frame_size: usize,
    ) -> Result<Self, anyhow::Error> {
        todo!()
    }
}

#[derive(Debug)]
pub struct BoneAnimation {
    /// Name of the bone this animation applies to.
    pub name: String,
    /// Index of the bone in the model that this animation applies to. This should be used for
    /// animation linking rather than the name.
    pub bone_index: usize,
    /// Unknown property from the animation header.
    pub unknown_prop: Option<u32>,
}

impl BoneAnimation {
    fn parse_v1(mut chunks: ChunksIter, num_frames: usize) -> anyhow::Result<Self> {
        const HEADER: &str = "Parsing the bone data header (chunk 0x00001003)";
        mini_chunk_map! {
            Header {
                [ids::ANIMATION_BONE_DATA_HEADER_NAME] name: String,
                [ids::ANIMATION_BONE_DATA_HEADER_IDX] bone_index: U32Usize,
                [ids::ANIMATION_BONE_DATA_HEADER_TRANS_OFFSET] trans_offset: Vec3,
                [ids::ANIMATION_BONE_DATA_HEADER_TRANS_SCALE] trans_scale: Vec3,
                [ids::ANIMATION_BONE_DATA_HEADER_SCALE_OFFSET] scale_offset: Vec3,
                [ids::ANIMATION_BONE_DATA_HEADER_SCALE_SCALE] scale_scale: Vec3,
                [ids::ANIMATION_BONE_DATA_HEADER_UNKNOWN] unknown_prop: u32 as optional,
            }
        }
        let Header {
            name,
            bone_index: U32Usize(bone_index),
            trans_offset,
            trans_scale,
            scale_offset,
            scale_scale,
            unknown_prop,
        } = chunks
            .expect_chunk(ids::ANIMATION_BONE_DATA_HEADER)
            .context(HEADER)?
            .data()
            .context(HEADER)?
            .as_mini_chunks()
            .expect_struct()
            .context(HEADER)?;

        Ok(Self {
            name,
            bone_index,
            unknown_prop,
        })
    }
}
