use petro_chunked::byteblock::ConvertBytes;
use petro_chunked::{
    FromByteBlock, FromChunks, FromMiniChunks, U8Bool, U32Bool, U32Usize, U32UsizeVec,
    iter_chunks, iter_mini_chunks,
};

/// Helper to build a full 32-bit chunk buffer in tests.
fn make_chunk(id: u32, is_container: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&id.to_le_bytes());
    let mut size = payload.len() as u32;
    if is_container {
        size |= 0x8000_0000;
    }
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Helper to build an 8-bit mini-chunk buffer in tests.
fn make_mini_chunk(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len());
    out.push(id);
    out.push(payload.len() as u8);
    out.extend_from_slice(payload);
    out
}

#[test]
fn test_derive_from_byte_block() {
    #[derive(FromByteBlock, Debug, PartialEq)]
    #[byte_block(pad_after = 4)]
    struct Header<'b> {
        #[byte_block(wire_format = U32Usize)]
        count: usize,
        unknown_flags: u16,
        #[byte_block(pad_after = 2)]
        name: &'b str,
        #[byte_block(wire_format = U32Bool)]
        hidden: bool,
        #[byte_block(wire_format = U8Bool)]
        in_bursts: bool,
        coords: [u16; 2],
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&0xBBAAu16.to_le_bytes()); // named unknown field
    bytes.extend_from_slice(b"test\0");
    bytes.extend_from_slice(&[0xCC, 0xDD]); // pad_after = 2
    bytes.extend_from_slice(&1u32.to_le_bytes()); // U32Bool -> true
    bytes.push(1u8); // U8Bool -> true
    bytes.extend_from_slice(&10u16.to_le_bytes());
    bytes.extend_from_slice(&20u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]); // container pad_after = 4

    let header: Header<'_> = bytes.as_slice().convert().unwrap();
    assert_eq!(
        header,
        Header {
            count: 7,
            unknown_flags: 0xBBAA,
            name: "test",
            hidden: true,
            in_bursts: true,
            coords: [10, 20],
        }
    );
}

#[test]
fn test_derive_from_mini_chunks_map_and_ignore_unknown() {
    #[derive(FromMiniChunks, Debug, PartialEq)]
    #[mini_chunk(map, ignore_unknown)]
    struct ProxyConn<'b> {
        #[mini_chunk(0x05)]
        name: &'b str,
        #[mini_chunk(0x06, wire_format = U32Usize)]
        bone: usize,
        #[mini_chunk(0x07, default, wire_format = U32Bool)]
        hidden: bool,
        #[mini_chunk(0x08, default = true, wire_format = U32Bool)]
        alt_decrease_stay_hidden: bool,
        #[mini_chunk(0x09, optional, wire_format = U8Bool)]
        in_bursts: Option<bool>,
        #[mini_chunk(0x0a, repeated)]
        tags: Vec<u16>,
    }

    // Out-of-order mini-chunks with an unknown mini-chunk (0xfe) and omitted defaults.
    let mut data = Vec::new();
    data.extend(make_mini_chunk(0x0a, &100u16.to_le_bytes()));
    data.extend(make_mini_chunk(0x06, &3u32.to_le_bytes()));
    data.extend(make_mini_chunk(0xfe, &[1, 2, 3, 4, 5])); // unknown, should be ignored
    data.extend(make_mini_chunk(0x09, &[1u8]));
    data.extend(make_mini_chunk(0x05, b"p_smoke_ALT1\0"));
    data.extend(make_mini_chunk(0x0a, &200u16.to_le_bytes()));

    let parsed: ProxyConn<'_> = iter_mini_chunks(&data).expect_struct().unwrap();
    assert_eq!(
        parsed,
        ProxyConn {
            name: "p_smoke_ALT1",
            bone: 3,
            hidden: false,
            alt_decrease_stay_hidden: true,
            in_bursts: Some(true),
            tags: vec![100, 200],
        }
    );
}

#[test]
fn test_derive_from_mini_chunks_sequence() {
    #[derive(FromMiniChunks, Debug, PartialEq)]
    #[mini_chunk(sequence)]
    struct AnimHeader {
        #[mini_chunk(0x01, wire_format = U32Usize)]
        num_frames: usize,
        #[mini_chunk(0x02)]
        fps: f32,
        #[mini_chunk(0x03, wire_format = U32Usize)]
        num_bones: usize,
        #[mini_chunk(0x0b, optional, wire_format = U32Usize)]
        rot_frame_size: Option<usize>,
    }

    let mut data = Vec::new();
    data.extend(make_mini_chunk(0x01, &60u32.to_le_bytes()));
    data.extend(make_mini_chunk(0x02, &30.0f32.to_le_bytes()));
    data.extend(make_mini_chunk(0x03, &12u32.to_le_bytes()));

    let header: AnimHeader = iter_mini_chunks(&data).expect_struct().unwrap();
    assert_eq!(
        header,
        AnimHeader {
            num_frames: 60,
            fps: 30.0,
            num_bones: 12,
            rot_frame_size: None,
        }
    );
}

#[test]
fn test_derive_from_chunks_submesh_pairs_and_enums() {
    #[derive(FromMiniChunks, Debug, PartialEq)]
    #[mini_chunk(sequence)]
    struct ParamEntry<'b, T: FromByteBlock<'b>> {
        #[mini_chunk(0x01)]
        name: &'b str,
        #[mini_chunk(0x02)]
        value: T,
    }

    #[derive(FromChunks, Debug, PartialEq)]
    enum RawShaderParam<'b> {
        #[chunk(0x0001_0102, mini_chunks)]
        Int(ParamEntry<'b, i32>),
        #[chunk(0x0001_0103, mini_chunks)]
        Float(ParamEntry<'b, f32>),
        #[chunk(0x0001_0105, mini_chunks)]
        Texture(ParamEntry<'b, &'b str>),
    }

    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct RawShader<'b> {
        #[chunk(0x0001_0101)]
        name: &'b str,
        #[chunk(stream, repeated)]
        params: Vec<RawShaderParam<'b>>,
    }

    #[derive(FromChunks, Debug, PartialEq)]
    enum RawVertexBuf {
        #[chunk(0x0001_0005)]
        V1(Vec<u32>),
        #[chunk(0x0001_0007)]
        V2(Vec<u16>),
    }

    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(map)]
    struct RawSubmeshData<'b> {
        #[chunk(0x0001_0002)]
        vertex_format: &'b str,
        #[chunk(0x0001_0004)]
        indices: Vec<[u16; 3]>,
        #[chunk(0x0001_0006, wire_format = U32UsizeVec, default)]
        bone_mappings: Vec<usize>,
        #[chunk(stream)]
        vertices: RawVertexBuf,
    }

    /// Multi-chunk sequence struct: each submesh is a pair of sibling chunks `(0x10100, 0x10000)`.
    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct RawSubmesh<'b> {
        #[chunk(0x0001_0100, children)]
        material: RawShader<'b>,
        #[chunk(0x0001_0000, children)]
        data: RawSubmeshData<'b>,
    }

    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct RawMesh<'b> {
        #[chunk(0x0000_0401)]
        name: &'b str,
        #[chunk(stream, repeated)]
        submeshes: Vec<RawSubmesh<'b>>,
    }

    // Build a material chunk (0x10100)
    let mut mat_children = Vec::new();
    mat_children.extend(make_chunk(0x0001_0101, false, b"MeshAdditive.fx\0"));
    let mut param1 = Vec::new();
    param1.extend(make_mini_chunk(0x01, b"BaseTexture\0"));
    param1.extend(make_mini_chunk(0x02, b"ship.dds\0"));
    mat_children.extend(make_chunk(0x0001_0105, false, &param1));

    // Build a submesh data chunk (0x10000) with out-of-order children (V2 vertices before indices)
    let mut data_children = Vec::new();
    data_children.extend(make_chunk(0x0001_0007, false, &[1, 0, 2, 0, 3, 0])); // V2 u16s
    data_children.extend(make_chunk(0x0001_0006, false, &[4, 0, 0, 0, 9, 0, 0, 0])); // U32UsizeVec -> [4, 9]
    data_children.extend(make_chunk(0x0001_0002, false, b"alD3dVertNU2\0"));
    data_children.extend(make_chunk(0x0001_0004, false, &[0, 0, 1, 0, 2, 0])); // 1 triangle [0, 1, 2]

    // Build the outer Mesh chunk contents: name + 2 submesh (material, data) pairs
    let mut mesh_children = Vec::new();
    mesh_children.extend(make_chunk(0x0000_0401, false, b"Hull_LOD0\0"));
    mesh_children.extend(make_chunk(0x0001_0100, true, &mat_children));
    mesh_children.extend(make_chunk(0x0001_0000, true, &data_children));
    mesh_children.extend(make_chunk(0x0001_0100, true, &mat_children));
    mesh_children.extend(make_chunk(0x0001_0000, true, &data_children));

    let mesh: RawMesh<'_> = iter_chunks(&mesh_children).expect_struct().unwrap();
    assert_eq!(mesh.name, "Hull_LOD0");
    assert_eq!(mesh.submeshes.len(), 2);
    assert_eq!(mesh.submeshes[0].material.name, "MeshAdditive.fx");
    assert_eq!(
        mesh.submeshes[0].material.params,
        vec![RawShaderParam::Texture(ParamEntry {
            name: "BaseTexture",
            value: "ship.dds",
        })]
    );
    assert_eq!(mesh.submeshes[0].data.vertex_format, "alD3dVertNU2");
    assert_eq!(mesh.submeshes[0].data.indices, vec![[0, 1, 2]]);
    assert_eq!(mesh.submeshes[0].data.bone_mappings, vec![4, 9]);
    assert_eq!(
        mesh.submeshes[0].data.vertices,
        RawVertexBuf::V2(vec![1, 2, 3])
    );
}

#[test]
fn test_derive_particle_heterogeneous_tracks_with_same_chunk_ids() {
    #[derive(FromMiniChunks, Debug, PartialEq)]
    #[mini_chunk(sequence)]
    struct TrackProps<T: for<'b> FromByteBlock<'b>> {
        #[mini_chunk(0x02)]
        first_key: T,
        #[mini_chunk(0x03)]
        last_key: T,
        #[mini_chunk(0x04)]
        interpolation: u32,
    }

    #[derive(FromMiniChunks, Debug, PartialEq)]
    #[mini_chunk(map)]
    struct TrackData<T: for<'b> FromByteBlock<'b>> {
        #[mini_chunk(0x05, repeated)]
        keys: Vec<(T, f32)>,
    }

    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct ByteTrack {
        #[chunk(0x0, mini_chunks)]
        props: TrackProps<u8>,
        #[chunk(0x1, mini_chunks)]
        data: TrackData<u32>,
    }

    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct FloatTrack {
        #[chunk(0x0, mini_chunks)]
        props: TrackProps<f32>,
        #[chunk(0x1, mini_chunks)]
        data: TrackData<f32>,
    }

    /// Like `alo-particle`'s emitter tracks chunk (0x1): positional tracks that all share chunk
    /// IDs (0x0, 0x1) but decode different types (`ByteTrack` vs `FloatTrack`) depending on position!
    #[derive(FromChunks, Debug, PartialEq)]
    #[chunk(sequence)]
    struct EmitterTracks {
        #[chunk(stream)]
        red: ByteTrack,
        #[chunk(stream)]
        size: FloatTrack,
    }

    let mut red_props = Vec::new();
    red_props.extend(make_mini_chunk(0x02, &[255u8]));
    red_props.extend(make_mini_chunk(0x03, &[128u8]));
    red_props.extend(make_mini_chunk(0x04, &0u32.to_le_bytes()));

    let mut red_data = Vec::new();
    let mut key_bytes = Vec::new();
    key_bytes.extend_from_slice(&200u32.to_le_bytes());
    key_bytes.extend_from_slice(&0.5f32.to_le_bytes());
    red_data.extend(make_mini_chunk(0x05, &key_bytes));

    let mut size_props = Vec::new();
    size_props.extend(make_mini_chunk(0x02, &1.5f32.to_le_bytes()));
    size_props.extend(make_mini_chunk(0x03, &4.0f32.to_le_bytes()));
    size_props.extend(make_mini_chunk(0x04, &1u32.to_le_bytes()));

    let mut tracks_buf = Vec::new();
    tracks_buf.extend(make_chunk(0x0, false, &red_props));
    tracks_buf.extend(make_chunk(0x1, false, &red_data));
    tracks_buf.extend(make_chunk(0x0, false, &size_props));
    tracks_buf.extend(make_chunk(0x1, false, &[])); // empty keys

    let tracks: EmitterTracks = iter_chunks(&tracks_buf).expect_struct().unwrap();
    assert_eq!(tracks.red.props.first_key, 255);
    assert_eq!(tracks.red.props.last_key, 128);
    assert_eq!(tracks.red.data.keys, vec![(200, 0.5)]);
    assert_eq!(tracks.size.props.first_key, 1.5);
    assert_eq!(tracks.size.props.last_key, 4.0);
    assert!(tracks.size.data.keys.is_empty());
}
