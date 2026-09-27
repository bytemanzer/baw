use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context as _, bail};
use petro_meg::path::{MegPath, MegPathBuf, meg_path};
use petro_meg::reader::{FileEntry, MegReadOptions, ReadMegMeta};
use petro_meg::version::MegV1;
use tracing::debug;

use crate::assets::asset::AssetKind;
use crate::assets::{Asset, FileSystemView};
use crate::extraction::xmlmodel::FileListXml;
use crate::xml;

/// An asset backed by a MEGA file entry.
pub(crate) struct MegAsset {
    /// File entry identifying the region in the meg_file that this asset comes from.
    entry: FileEntry,
    /// Last modified time of the mega file, used for a best-effort concurrent modification check.
    expect_mtime: SystemTime,
    /// Path to the mega file that this asset is from.
    meg_file: Arc<Path>,
}

impl AssetKind for MegAsset {
    fn path(&self) -> &MegPath {
        self.entry.name()
    }

    fn open_reader(&self) -> io::Result<Box<dyn Read>> {
        static OPTIONS: MegReadOptions = MegReadOptions::new();
        let file = File::open(&self.meg_file)?;
        let mtime = file.metadata()?.modified()?;
        if mtime != self.expect_mtime {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "MEGA file modified concurrently",
            ));
        }
        Ok(Box::new(self.entry.extract_from(file, &OPTIONS)?))
    }

    fn modified_time(&self) -> io::Result<SystemTime> {
        // Report he expected mtime from when we loaded the mega file header, because any subsequent
        // modification breaks loads anyway.
        Ok(self.expect_mtime)
    }

    fn describe_source(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<MegAsset from {}>", self.meg_file.display())
    }
}

/// Iterator over all meg file assets found in a file system view.
pub(crate) struct MegAssets {
    /// Iterator over the real-path sources for the mega files.
    real_paths: std::vec::IntoIter<(MegPathBuf, Arc<Path>)>,
    current: Option<MegFileAssets>,
}

impl MegAssets {
    fn new(real_paths: Vec<(MegPathBuf, Arc<Path>)>) -> Self {
        Self {
            real_paths: real_paths.into_iter(),
            current: None,
        }
    }
}

impl Iterator for MegAssets {
    type Item = anyhow::Result<Box<Asset>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If there's no current meg file being processed, try to load the next one until we run
            // out of meg files to look for or hit an error.
            // If there is a meg file being processed, read entries from it until we run out, then
            // try to load the next file.
            match self.current {
                None => match self.real_paths.next() {
                    // No current and no additional paths, so iteration is totally done.
                    None => return None,
                    Some((asset_path, real_path)) => {
                        let meg_assets = MegFileAssets::new(real_path.clone()).with_context(|| {
                            format!(
                                "Loading the MEGA file for {asset_path} from {}",
                                real_path.display()
                            )
                        });
                        match meg_assets {
                            // Set current. Outer loop will take care of trying to fetch from it.
                            Ok(assets) => self.current = Some(assets),
                            Err(err) => return Some(Err(err)),
                        }
                    }
                },
                Some(ref mut current) => match current.next() {
                    // Clear current; outer loop will load the next file and retry. We can't return
                    // here because we may still have assets left in other meg files.
                    None => self.current = None,
                    Some(next) => return Some(Ok(Asset::new(next))),
                },
            }
        }
    }
}

/// Asset iterator for a single MEGA file.
struct MegFileAssets {
    /// Modification time of this MegaFileAssets. Used for best-effort concurrent modification
    /// checks.
    mtime: SystemTime,
    /// Path to the MEGA file.
    path: Arc<Path>,
    /// Iterator of remaining FileEntries to process.
    entries: std::vec::IntoIter<FileEntry>,
}

impl MegFileAssets {
    /// Create an iterator over the contents of a MEGA file at the given path.
    pub(crate) fn new(path: impl Into<Arc<Path>>) -> anyhow::Result<Self> {
        let path = path.into();
        let meg_file = File::open(&path)?;
        let mtime = meg_file.metadata()?.modified()?;
        let entries = MegV1.read_meg_meta(BufReader::new(meg_file))?.into_iter();
        Ok(Self {
            mtime,
            path,
            entries,
        })
    }
}

impl Iterator for MegFileAssets {
    type Item = MegAsset;

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|subfile| MegAsset {
            entry: subfile,
            expect_mtime: self.mtime,
            meg_file: self.path.clone(),
        })
    }
}

/// Find a data/megafiles.xml file in the current filesystem view, then finds any mega files it
/// lists which are present in the filesystem and iterates over their files.
pub(crate) fn find_mega_assets(filesystem: &FileSystemView) -> anyhow::Result<MegAssets> {
    const XML_PATH: &MegPath = meg_path!("Data\\Megafiles.XML");
    let megafiles_xml = match filesystem.get(XML_PATH) {
        Some(asset) => asset,
        None => bail!("{XML_PATH} not found"),
    };
    let megafiles_xml = megafiles_xml.open_reader()?;
    let megafiles: FileListXml = xml::deserialize_xml_utf8_or_cp1252(megafiles_xml)?;

    let mut meg_file_real_paths = Vec::<(MegPathBuf, Arc<Path>)>::new();
    for asset_path in megafiles.files {
        let source = match filesystem.get(&asset_path) {
            Some(source) => source,
            None => {
                debug!(
                    "Skipping MEGA file {asset_path} which does not exist in the current file \
                    overlays",
                );
                continue;
            }
        };
        match source.real_path() {
            Some(path) => meg_file_real_paths.push((asset_path, path.into())),
            None => bail!(
                "MEGA file {asset_path} must be a real on disk file, but was {}",
                source.describe_source()
            ),
        }
    }

    Ok(MegAssets::new(meg_file_real_paths))
}
