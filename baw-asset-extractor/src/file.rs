use std::fs::File;
use std::io;
use std::path::Path;

use walkdir::WalkDir;

use crate::{Asset, AssetFinder, AssetVisitor, path_starts_with_ignore_case};

struct FileAsset<'p> {
    full_path: &'p Path,
    relative_path: &'p Path,
}

impl<'p> Asset for FileAsset<'p> {
    fn path(&self) -> &Path {
        self.relative_path
    }

    type Reader<'r>
        = File
    where
        Self: 'r;

    fn open_reader(&mut self) -> io::Result<Self::Reader<'_>> {
        File::open(self.full_path)
    }
}

/// Finds assets by walking the directory tree of the `Data` folder in `GameData`.
pub struct FileAssetFinder<'p> {
    /// Path to the `GameData` folder.
    game_data: &'p Path,
}

impl<'p> FileAssetFinder<'p> {
    /// Make a file asset finder that searches the given directory.
    pub fn new(game_data: &'p Path) -> Self {
        Self { game_data }
    }
}

impl<'a> AssetFinder for FileAssetFinder<'a> {
    fn find_assets<V: AssetVisitor>(&self, visitor: &mut V) -> anyhow::Result<()> {
        let walker = WalkDir::new(self.game_data)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                let path = entry.path().strip_prefix(self.game_data).unwrap();
                path_starts_with_ignore_case(path, "Data")
            });
        for entry in walker {
            let entry = entry?;
            let relative_path = entry.path().strip_prefix(self.game_data).unwrap();
            let mut asset = FileAsset {
                full_path: entry.path(),
                relative_path,
            };
            visitor.visit_asset(&mut asset)?;
        }
        Ok(())
    }
}
