use std::fs::File;
use std::path::Path;

use anyhow::anyhow;
use tracing::info;

use crate::{Asset, AssetVisitor};

/// Visits all model files and writes them to the dest_folder, at the same relative path, but with
/// the local part converted to lowercase.
pub struct ModelVisitor<'a> {
    dest_folder: &'a Path,
}

impl<'a> ModelVisitor<'a> {
    /// Create a new model visitor that writes to the given Dest folder.
    pub fn new(dest_folder: &'a Path) -> Self {
        ModelVisitor { dest_folder }
    }
}

impl<'a> AssetVisitor for ModelVisitor<'a> {
    fn visit_asset<A: Asset>(&mut self, asset: &mut A) -> anyhow::Result<()> {
        if let Some(ext) = asset.path().extension() {
            if ext.eq_ignore_ascii_case("alo") || ext.eq_ignore_ascii_case("ala") {
                let mut out_file = asset.path().to_path_buf();
                out_file.as_mut_os_str().make_ascii_lowercase();
                let filename = out_file
                    .file_name()
                    .ok_or_else(|| anyhow!("Missing File Name"))?;
                let mut out_file = self.dest_folder.to_path_buf();
                if ext.eq_ignore_ascii_case("ala") {
                    out_file.push("animations");
                } else {
                    out_file.push("models");
                }
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
