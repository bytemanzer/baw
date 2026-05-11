use anyhow::Context;
use petro_chunked::ChunkContent;
use petro_chunked::byteblock::ConvertBytes;
use thiserror::Error;

use crate::ids;
use crate::primitives::{U32Bool, U32Usize};

/// Connections found in the ALO file.
pub(crate) struct Connections {
    pub(crate) objects: Vec<ObjectConnection>,
    pub(crate) proxies: Vec<ProxyConnection>,
}

impl Connections {
    pub(crate) fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut chunks = content.children().context("parsing connections")?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the connection counts (chunk 00000601)")]
        struct ConnCountCtx;
        let chunk = chunks
            .expect_chunk(ids::CONNECTION_COUNT)
            .context(ConnCountCtx)?;
        let ConnectionCount {
            object_count,
            proxy_count,
        } = ConnectionCount::parse(chunk.content()).context(ConnCountCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing object connection {object_idx} / {object_count}")]
        struct ObjectCtx {
            object_count: usize,
            object_idx: usize,
        }
        let mut objects = Vec::with_capacity(object_count);
        for object_idx in 0..object_count {
            let ctx = ObjectCtx {
                object_count,
                object_idx,
            };
            let chunk = chunks.expect_chunk(ids::CONNECTION_OBJECT).context(ctx)?;
            let object = ObjectConnection::parse(chunk.content()).context(ctx)?;
            objects.push(object)
        }

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing proxy connection {proxy_idx} / {proxy_count}")]
        struct ProxyCtx {
            proxy_count: usize,
            proxy_idx: usize,
        }

        let mut proxies = Vec::with_capacity(proxy_count);
        for proxy_idx in 0..proxy_count {
            let ctx = ProxyCtx {
                proxy_count,
                proxy_idx,
            };
            let chunk = chunks.expect_chunk(ids::CONNECTION_PROXY).context(ctx)?;
            let proxy = ProxyConnection::parse(chunk.content()).context(ctx)?;
            proxies.push(proxy);
        }

        chunks.expect_empty().context("after parsing connections")?;

        Ok(Self { objects, proxies })
    }
}

struct ConnectionCount {
    object_count: usize,
    proxy_count: usize,
}

impl ConnectionCount {
    fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut mini_chunks = content.data()?.as_mini_chunks();

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the object count (mini-chunk 01)")]
        struct ObjCountCtx;

        let U32Usize {
            value: object_count,
        } = mini_chunks
            .expect_chunk(ids::CONNECTION_COUNT_OBJECT)
            .context(ObjCountCtx)?
            .convert()
            .context(ObjCountCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy count (mini-chunk 04)")]
        struct ProxyCountCtx;

        let U32Usize { value: proxy_count } = mini_chunks
            .expect_chunk(ids::CONNECTION_COUNT_PROXY)
            .context(ProxyCountCtx)?
            .convert()
            .context(ProxyCountCtx)?;
        Ok(Self {
            object_count,
            proxy_count,
        })
    }
}

/// An object connection.
pub(crate) struct ObjectConnection {
    pub(crate) object: usize,
    pub(crate) bone: usize,
}

impl ObjectConnection {
    fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut mini_chunks = content.data()?.as_mini_chunks();

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the object count (mini-chunk 01)")]
        struct ObjIdxCtx;

        let U32Usize { value: object } = mini_chunks
            .expect_chunk(ids::CONNECTION_OBJECT_OBJ)
            .context(ObjIdxCtx)?
            .convert()
            .context(ObjIdxCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy count (mini-chunk 04)")]
        struct BoneIdxCtx;

        let U32Usize { value: bone } = mini_chunks
            .expect_chunk(ids::CONNECTION_OBJECT_BONE)
            .context(BoneIdxCtx)?
            .convert()
            .context(BoneIdxCtx)?;
        Ok(Self { object, bone })
    }
}

/// A proxy connection.
pub(crate) struct ProxyConnection {
    pub(crate) proxy_name: String,
    pub(crate) bone: usize,
    pub(crate) hidden: bool,
    pub(crate) alt_decrease_stay_hidden: bool,
}

impl ProxyConnection {
    fn parse(content: ChunkContent) -> anyhow::Result<Self> {
        let mut mini_chunks = content.data()?.as_mini_chunks();

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy name (mini-chunk 05)")]
        struct PNameCtx;
        let proxy_name = mini_chunks
            .expect_chunk(ids::CONNECTION_PROXY_NAME)
            .context(PNameCtx)?
            .convert()
            .context(PNameCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy bone (mini-chunk 06)")]
        struct PBoneCtx;
        let U32Usize { value: bone } = mini_chunks
            .expect_chunk(ids::CONNECTION_PROXY_BONE)
            .context(PBoneCtx)?
            .convert()
            .context(PBoneCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy hidden (mini-chunk 07)")]
        struct PHiddenCtx;
        let U32Bool { value: hidden } = mini_chunks
            .try_expect_chunk(ids::CONNECTION_PROXY_HIDDEN)
            .context(PHiddenCtx)?
            .map(|data| data.convert())
            .unwrap_or(Ok(U32Bool { value: false }))
            .context(PHiddenCtx)?;

        #[derive(Error, Debug, Copy, Clone)]
        #[error("parsing the proxy alt-decrease-stay-hidden (mini-chunk 08)")]
        struct PADSHCtx;
        let U32Bool {
            value: alt_decrease_stay_hidden,
        } = mini_chunks
            .try_expect_chunk(ids::CONNECTION_PROXY_ADSH)
            .context(PADSHCtx)?
            .map(|data| data.convert())
            .unwrap_or(Ok(U32Bool { value: false }))
            .context(PADSHCtx)?;

        Ok(Self {
            proxy_name,
            bone,
            hidden,
            alt_decrease_stay_hidden,
        })
    }
}
