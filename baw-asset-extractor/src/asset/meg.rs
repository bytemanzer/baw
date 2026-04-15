use std::io::{self, Read, Seek};
use std::path::Path;

use petro_meg::reader::{FileEntry, FileSegmentReader, MegReadOptions};

use super::{Asset, AssetFinder, AssetVisitor};

/// An asset backed by a MEGA file entry.
struct MegAsset<'r, R> {
    entry: FileEntry,
    reader: &'r mut R,
}

impl<'r, R: Read + Seek> Asset for MegAsset<'r, R> {
    fn path(&self) -> &Path {
        self.entry.name().as_str().as_ref()
    }

    type Reader<'s>
        = FileSegmentReader<'s>
    where
        Self: 's;

    fn open_reader(&mut self) -> io::Result<Self::Reader<'_>> {
        static OPTIONS: MegReadOptions = MegReadOptions::new();
        self.entry.extract_from(&mut *self.reader, &OPTIONS)
    }
}

pub(super) struct MegAssetFinder<'p> {
    game_data: &'p Path,
}

impl<'p> MegAssetFinder<'p> {
    /// Make a megafile asset finder that searches the given directory.
    pub(super) fn new(game_data: &'p Path) -> Self {
        Self { game_data }
    }
}

impl<'p> AssetFinder for MegAssetFinder<'p> {
    fn find_assets<V: AssetVisitor>(&self, visitor: &mut V) -> anyhow::Result<()> {

        Ok(())
    }
}
