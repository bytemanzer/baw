use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use petro_alo::AlaFile;
use petro_alo::animations::BoneAnimation;

#[derive(Parser)]
struct Args {
    /// The ALA files to parse.
    files: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut all_unk = BTreeSet::new();
    for file in &args.files {
        let content =
            std::fs::read(file).with_context(|| format!("parsing file {}", file.display()))?;
        let ala =
            AlaFile::parse(&content).with_context(|| format!("parsing file {}", file.display()))?;
        println!("{}: {}fps", file.display(), ala.fps);
        show_bone_anims(&ala.bone_animations, &mut all_unk, Indent::new(4));
    }
    println!("All discovered values of the unknown property:");
    for unk in all_unk.into_iter() {
        println!("    {unk:?}");
    }
    Ok(())
}

fn show_bone_anims(anims: &[BoneAnimation], all_unk: &mut BTreeSet<Option<u32>>, indent: Indent) {
    for bone in anims {
        all_unk.insert(bone.unknown_prop);
        println!(
            "{indent}{} ({}): unk={:?}",
            bone.name, bone.bone_index, bone.unknown_prop
        )
    }
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
