use anyhow::Context;
use glam::{Mat4, Vec4};
use petro_chunked::ChunkContent;
use petro_chunked::byteblock::{ConvertBytes, FromByteBlock};
use thiserror::Error;

use crate::error::FormatError;
use crate::ids;
use crate::alternatives::Alt;
use crate::primitives::{U32Bool, U32Usize};

/// How a bone should apply billboarding.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum BillboardMode {
    /// No billboarding.
    #[default]
    Disable = 0,
    /// Rotate so the negative Y axis faces the camera.
    Parallel = 1,
    /// Effectively identical to Parallel.
    Face = 2,
    /// Rotate around only the Z-Axis to face the camera.
    ZAxisView = 3,
    /// Rotate only around the Z-Axis but face the light instead of the camera.
    ZAxisLight = 4,
    /// Rotate only around the Z-Axis, but parallel to the wind instead of the camera.
    ZAxisWind = 5,
    /// Used to orient the texture used for the "glow" of "atmosphere" on planets in e.g. the
    /// galactic view. Rotates around the root rather than its own center.
    SunlightGlow = 6,
    /// Rotates a plane positioned at <x, 0, 0> so that its local -Y faces the camera, then rotates
    /// around the parent Z and X to position it in the correct spot in the sky for the light
    /// direction.
    Sun = 7,
}

impl<'b> FromByteBlock<'b> for BillboardMode {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), petro_chunked::ChunkError> {
        let (mode, rest) = u32::take_from(bytes)?;
        let mode = match mode {
            0 => Self::Disable,
            1 => Self::Parallel,
            2 => Self::Face,
            3 => Self::ZAxisView,
            4 => Self::ZAxisLight,
            5 => Self::ZAxisWind,
            6 => Self::SunlightGlow,
            7 => Self::Sun,
            _ => Err(FormatError::UnrecognizedBillboardMode { mode })?,
        };
        Ok((mode, rest))
    }
}

/// A proxy object that gets loaded and automatically attached to a bone.
#[derive(Debug)]
pub struct Proxy {
    /// Name of the proxy object to attach.
    pub name: String,
    /// Alt level that this proxy should be shown at. If None, this proxy does not respond to alt
    /// leveling.
    pub alt: Option<Alt>,
    /// Whether this proxy is initially hidden.
    pub hidden: bool,
    /// For an ALT proxy, don't make the proxy visible when the alt-level changes to the target alt
    /// by decreasing.
    pub alt_decrease_stay_hidden: bool,
}

/// A bone in an ALO file.
#[derive(Debug)]
pub struct Bone {
    /// Name of the bone.
    pub name: String,
    /// Index of the parent bone, if any.
    pub parent: Option<usize>,
    /// Whether things attached to the bone are visible.
    pub visible: bool,
    /// TODO: figure out how to interpet the billboard flags.
    pub billboard: BillboardMode,
    /// Matrix which transforms from this bone to the parent bone.
    pub local_to_parent: Mat4,
    /// Matrix which transforms from this bone to global space.
    pub local_to_global: Mat4,
    /// List of proxies that should attach to this bone.
    pub proxies: Vec<Proxy>,
}

impl Bone {
    fn parse(content: ChunkContent, existing_bones: &[Bone]) -> anyhow::Result<Self> {
        let mut chunks = content.children()?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the bone name (chunk 00000203)")]
        struct BoneNameCtx;
        let name = chunks
            .expect_chunk(ids::BONE_NAME)
            .context(BoneNameCtx)?
            .data()
            .context(BoneNameCtx)?
            .convert()
            .context(BoneNameCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the bone data (chunk 00000205 or 00000206)")]
        struct BoneDataCtx;

        let bone_data = chunks
            .expect_chunk([ids::BONE_V1, ids::BONE_V2])
            .context(BoneDataCtx)?;
        let content = bone_data.data().context(BoneDataCtx)?.as_data();
        let (parent, U32Bool(visible), billboard, matrix): (u32, _, _, [[f32; 4]; 3]) =
            match bone_data.id() {
                ids::BONE_V1 => {
                    let (parent, visible, matrix) = content.convert().context(BoneDataCtx)?;
                    (parent, visible, BillboardMode::Disable, matrix)
                }
                ids::BONE_V2 => content.convert().context(BoneDataCtx)?,
                _ => unreachable!(),
            };
        let (parent, parent_to_global) = match parent {
            u32::MAX => (None, Mat4::IDENTITY),
            parent => {
                let parent: usize = parent
                    .try_into()
                    .context("converting parent index to usize")?;
                if parent >= existing_bones.len() {
                    Err(FormatError::MisorderedBone {
                        idx: existing_bones.len(),
                        parent,
                    })?;
                }
                (Some(parent), existing_bones[parent].local_to_global)
            }
        };
        let local_to_parent = Mat4::from_cols(
            Vec4::from_array(matrix[0]),
            Vec4::from_array(matrix[1]),
            Vec4::from_array(matrix[2]),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        )
        .transpose();
        let local_to_global = parent_to_global * local_to_parent;

        chunks
            .expect_empty()
            .context("after parsing the bone content")?;

        Ok(Bone {
            name,
            parent,
            visible,
            billboard,
            local_to_parent,
            local_to_global,
            proxies: Vec::new(),
        })
    }
}

/// The Skeleton from an ALO file.
#[derive(Default, Debug)]
pub struct Skeleton {
    /// The bones that make up the skeleton.
    pub bones: Vec<Bone>,
}

impl Skeleton {
    pub(crate) fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut chunks = content.children().context("parsing skeleton")?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the bone count (chunk 00000201)")]
        struct BoneCountCtx;

        let (U32Usize(bone_count), _pad): (_, &[u8; 124]) = chunks
            .expect_chunk(ids::BONE_COUNT)
            .context(BoneCountCtx)?
            .data()
            .context(BoneCountCtx)?
            .convert()
            .context(BoneCountCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing bone {bone_idx} / {bone_count}")]
        struct BoneCtx {
            bone_count: usize,
            bone_idx: usize,
        }

        let mut bones = Vec::with_capacity(bone_count);
        for bone_idx in 0..bone_count {
            let ctx = BoneCtx {
                bone_count,
                bone_idx,
            };
            let chunk = chunks.expect_chunk(ids::BONE).context(ctx)?;
            let bone = Bone::parse(chunk.content(), &bones).context(ctx)?;

            bones.push(bone);
        }
        chunks
            .expect_empty()
            .context("after parsing the expected number of bones")?;

        Ok(Self { bones })
    }
}
