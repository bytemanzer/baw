use std::fs::File;
use std::path::Path;

use anyhow::anyhow;
use tracing::info;

use crate::{Asset, AssetVisitor};

/// Visits all model files and writes them to the dest_folder, at the same relative path, but with
/// the local part converted to lowercase.
pub struct TextureVisitor<'a> {
    dest_folder: &'a Path,
}

impl<'a> TextureVisitor<'a> {
    /// Create a new model visitor that writes to the given Dest folder.
    pub fn new(dest_folder: &'a Path) -> Self {
        TextureVisitor { dest_folder }
    }
}

impl<'a> AssetVisitor for TextureVisitor<'a> {
    fn visit_asset<A: Asset>(&mut self, asset: &mut A) -> anyhow::Result<()> {
        if let Some(ext) = asset.path().extension() {
            if ext.eq_ignore_ascii_case("tga") || ext.eq_ignore_ascii_case("dds") {
                let mut out_file = asset.path().to_path_buf();
                out_file.as_mut_os_str().make_ascii_lowercase();
                let filename = out_file
                    .file_name()
                    .ok_or_else(|| anyhow!("Missing File Name"))?;
                let mut out_file = self.dest_folder.join("textures");
                out_file.push(filename);
                if let Some(out_dir) = out_file.parent() {
                    std::fs::create_dir_all(out_dir)?;
                }
                info!(
                    "Copying {} to {}",
                    asset.path().display(),
                    out_file.display()
                );
                let mut out_file = File::create(out_file)?;
                let mut source = asset.open_reader()?;
                std::io::copy(&mut source, &mut out_file)?;
            }
        }

        Ok(())
    }
}
