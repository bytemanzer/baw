use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

use anyhow::Context;
use bitflags::bitflags;
use clap::Parser;
use glam::Vec4Swizzles;
use petro_alo::AloFile;
use petro_alo::mesh::{Mesh, Shader, ShaderParam, Submesh, VertexFormat};
use petro_alo::skeleton::Bone;

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
    struct VertexFields: u32 {
        const POS = 0x0000_0001;
        const NORM = 0x0000_0002;
        const TC0 = 0x0000_0004;
        const TC1 = 0x0000_0008;
        const TC2 = 0x0000_0010;
        const TC3 = 0x0000_0020;
        const TC4 = 0x0000_0040;
        const TAN = 0x0000_0080;
        const BIN = 0x0000_0100;
        const COL = 0x0000_0200;
        const BIDX = 0x0000_0400;
        const BWT = 0x0000_0800;
        const BMAP = 0x0000_1000;
    }
}

#[derive(Parser)]
struct Args {
    /// The ALO files to parse.
    files: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut formats = BTreeMap::new();
    let mut shaders = BTreeMap::new();
    let mut params = BTreeMap::new();
    for file in &args.files {
        let content =
            std::fs::read(file).with_context(|| format!("parsing file {}", file.display()))?;
        let alo =
            AloFile::parse(&content).with_context(|| format!("parsing file {}", file.display()))?;
        println!("{}:", file.display());
        show_model(
            &alo,
            &mut formats,
            &mut shaders,
            &mut params,
            Indent::new(4),
        );
    }
    println!("All discovered vertex formats:");
    for (format, fields) in formats {
        println!("{format}: {:?}", format.parse::<VertexFormat>().unwrap());
        for fmt in fields {
            println!("    {fmt:?}");
        }
    }
    println!("All discovered shaders");
    for (name, params) in shaders {
        println!("    {name}: {params:?}");
    }
    println!("All discovered shader params");
    for (name, formats) in params {
        println!("    {name}: {formats:?}");
    }
    Ok(())
}

fn show_model(
    model: &AloFile,
    formats: &mut BTreeMap<String, BTreeSet<VertexFields>>,
    shaders: &mut BTreeMap<String, BTreeSet<String>>,
    params: &mut BTreeMap<String, BTreeSet<&'static str>>,
    indent: Indent,
) {
    println!("{indent}Skeleton:");
    show_bones(&model.skeleton.bones, None, indent.bump());
    println!("{indent}Meshes:");
    show_meshes(
        &model.meshes,
        &model.skeleton.bones,
        formats,
        shaders,
        params,
        indent.bump(),
    );
}

fn show_bones(bones: &[Bone], parent: Option<usize>, indent: Indent) {
    for (idx, bone) in bones.iter().enumerate() {
        if bone.parent == parent {
            println!(
                "{indent}{}: vis={} bb={:?} pos={:?}",
                bone.name,
                bone.visible,
                bone.billboard,
                bone.local_to_global.w_axis.xyz()
            );
            show_bones(bones, Some(idx), indent.bump());
        }
    }
}

fn show_meshes(
    meshes: &[Mesh],
    bones: &[Bone],
    formats: &mut BTreeMap<String, BTreeSet<VertexFields>>,
    shaders: &mut BTreeMap<String, BTreeSet<String>>,
    params: &mut BTreeMap<String, BTreeSet<&'static str>>,
    indent: Indent,
) {
    for mesh in meshes {
        print!("{indent}{}:", mesh.name);
        if mesh.hidden {
            print!(" Hidden");
        }
        if mesh.collision_enabled {
            print!(" Collider");
        }
        match mesh.parent_bone {
            Some(bidx) => println!(" [{}]", bones[bidx].name),
            None => println!(),
        }
        let indent = indent.bump();
        for (idx, submesh) in mesh.submeshes.iter().enumerate() {
            println!(
                "{indent}Submesh {idx: >2}: vertfmt={:?}",
                submesh.vertex_format
            );
            show_shader_params(&submesh.shader, shaders, params, indent.bump());
            formats
                .entry(submesh.vertex_format_string.clone())
                .or_default()
                .insert(find_used_vertex_values(submesh));
        }
    }
}

fn show_shader_params(
    shader: &Shader,
    shaders: &mut BTreeMap<String, BTreeSet<String>>,
    params: &mut BTreeMap<String, BTreeSet<&'static str>>,
    indent: Indent,
) {
    println!("{indent}Shader {}", shader.filename);
    let shader_param_set = shaders.entry(shader.filename.clone()).or_default();
    let indent = indent.bump();
    for (name, value) in &shader.params {
        println!("{indent}{}: {:?}", name, value);
        let ty = match value {
            ShaderParam::Int(_) => "Int",
            ShaderParam::Float(_) => "Float",
            ShaderParam::Float3(_) => "Float3",
            ShaderParam::Texture(_) => "Texture",
            ShaderParam::Float4(_) => "Float4",
        };
        shader_param_set.insert(name.clone());
        params.entry(name.clone()).or_default().insert(ty);
    }
}

fn find_used_vertex_values(submesh: &Submesh) -> VertexFields {
    let mut fields = VertexFields::empty();
    if submesh.positions.iter().any(|&p| p != [0.0, 0.0, 0.0]) {
        fields |= VertexFields::POS;
    }
    if submesh.normals.iter().any(|&n| n != [0.0, 0.0, 0.0]) {
        fields |= VertexFields::NORM;
    }
    if submesh.tex_coords[0].iter().any(|&t| t != [0.0, 0.0]) {
        fields |= VertexFields::TC0;
    }
    if submesh.tex_coords[1].iter().any(|&t| t != [0.0, 0.0]) {
        fields |= VertexFields::TC1;
    }
    if submesh.tex_coords[2].iter().any(|&t| t != [0.0, 0.0]) {
        fields |= VertexFields::TC2;
    }
    if submesh.tex_coords[3].iter().any(|&t| t != [0.0, 0.0]) {
        fields |= VertexFields::TC3;
    }
    if submesh.tangents.iter().any(|&t| t != [0.0, 0.0, 0.0]) {
        fields |= VertexFields::TAN;
    }
    if submesh.binormals.iter().any(|&t| t != [0.0, 0.0, 0.0]) {
        fields |= VertexFields::BIN;
    }
    if submesh.colors.iter().any(|&t| t != [0.0, 0.0, 0.0, 0.0]) {
        fields |= VertexFields::COL;
    }
    if submesh.bone_indices.iter().any(|&t| t != [0, 0, 0, 0]) {
        fields |= VertexFields::BIDX;
    }
    if submesh
        .bone_weights
        .iter()
        .any(|&t| t != [0.0, 0.0, 0.0, 0.0])
    {
        fields |= VertexFields::BWT;
    }
    if !submesh.bone_mappings.is_empty() {
        fields |= VertexFields::BMAP;
    }
    fields
}

#[derive(Copy, Clone, Debug)]
struct Indent {
    depth: usize,
}

impl Indent {
    fn new(depth: usize) -> Self {
        Self { depth }
    }

    fn bump(&self) -> Indent {
        Indent {
            depth: self.depth + 4,
        }
    }
}

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SPACE: &'static str = "                    ";
        for idx in (0..self.depth).step_by(SPACE.len()) {
            let n = (self.depth - idx).min(SPACE.len());
            f.write_str(&SPACE[..n])?;
        }
        Ok(())
    }
}
