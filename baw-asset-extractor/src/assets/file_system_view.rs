use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::{Entry, Values};
use std::iter::FusedIterator;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::Arc;

use petro_meg::path::{MegPath, MegPathBuf};
use tracing::trace;

use crate::assets::Asset;

/// Represents a view of the state of the filesystem containing assets from some set of sources.
#[derive(Default)]
pub struct FileSystemView {
    view: BTreeMap<MegPathBuf, Box<Asset>>,
}

/// Create a new empty FileSystemView.
impl FileSystemView {
    /// Create a new empty FileSystemView.
    pub fn new() -> Self {
        Default::default()
    }

    /// Gets the asset at the given path.
    pub fn get(&self, path: &MegPath) -> Option<&Asset> {
        self.view.get(path).map(|a| &**a)
    }

    /// Inserts or overlays the given asset.
    pub fn overlay_asset(&mut self, asset: Box<Asset>) {
        match self.view.entry(asset.path().to_owned()) {
            Entry::Occupied(mut entry) => {
                trace!(
                    "Overlaying asset {}: replacing {} with {}",
                    entry.key(),
                    entry.get().describe_source(),
                    asset.describe_source(),
                );
                entry.insert(asset);
            }
            Entry::Vacant(entry) => {
                trace!(
                    "Inserting asset {}: {}",
                    entry.key(),
                    asset.describe_source(),
                );
                entry.insert(asset);
            }
        }
    }

    /// Explores all files in the given `GameData` directory and overlays all files within the
    /// `Data` folder onto this view.
    ///
    /// In the event of an error, the view is in an unspecified state (may be partially modified).
    pub fn overlay_disk_files(&mut self, dir: impl Into<Arc<Path>>) -> anyhow::Result<()> {
        for asset in super::file::find_asset_files(dir) {
            self.overlay_asset(asset?)
        }
        Ok(())
    }

    /// Finds the data/megafiles.xml file inside the existing file system view, then loads all
    /// megafiles defined in that file.
    pub fn overlay_mega_files(&mut self) -> anyhow::Result<()> {
        for asset in super::meg::find_mega_assets(self)? {
            self.overlay_asset(asset?);
        }
        Ok(())
    }

    /// Gets an iterator over the assets in the current file system view.
    pub fn assets(&self) -> Assets<'_> {
        Assets {
            inner: self.view.values(),
        }
    }

    /// Gets an iterator over a range of assets, by file name.
    pub fn asset_range<T, R>(&self, range: R) -> Range<'_>
    where
        T: Ord + ?Sized,
        MegPathBuf: Borrow<T> + Ord,
        R: RangeBounds<T>,
    {
        Range {
            inner: self.view.range(range),
        }
    }
}

/// An iterator over the assets in the current file system view.
pub struct Assets<'a> {
    inner: Values<'a, MegPathBuf, Box<Asset>>,
}

impl<'a> Iterator for Assets<'a> {
    type Item = &'a Asset;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|a| &**a)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    fn count(self) -> usize {
        self.inner.count()
    }

    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        F: FnMut(B, Self::Item) -> B,
    {
        self.inner.fold(init, |b, a| f(b, &**a))
    }
}

impl<'a> ExactSizeIterator for Assets<'a> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a> FusedIterator for Assets<'a> {}

/// Iterator over a range of assets.
#[derive(Default, Clone, Debug)]
pub struct Range<'a> {
    inner: std::collections::btree_map::Range<'a, MegPathBuf, Box<Asset>>,
}

impl<'a> Iterator for Range<'a> {
    type Item = &'a Asset;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, v)| &**v)
    }

    fn last(self) -> Option<Self::Item> {
        self.inner.last().map(|(_, v)| &**v)
    }
}

impl<'a> DoubleEndedIterator for Range<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|(_, v)| &**v)
    }
}

impl<'a> FusedIterator for Range<'a> {}
