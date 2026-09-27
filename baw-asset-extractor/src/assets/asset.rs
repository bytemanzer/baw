use std::fmt;
use std::io::{self, Read};
use std::path::Path;
use std::time::SystemTime;

use petro_meg::path::MegPath;

/// Represents a file within the game, possibly independent, possibly packed in a MEGA file.
pub(crate) trait AssetKind: 'static + Send + Sync {
    /// Gets the relative path of the Asset within the game, as a case-insensitive path.
    fn path(&self) -> &MegPath;

    /// Opens a new reader which can be used to read the content of this asset.
    fn open_reader(&self) -> io::Result<Box<dyn Read>>;

    /// Get the last modified time of this asset. May be used to skip copying or processing the
    /// asset if the destination is newer.
    fn modified_time(&self) -> io::Result<SystemTime>;

    /// If this Asset describes an on-disk file, this is the path to that file. None for file
    /// sources that have some transformation applied, such as MEGA file contents, which are
    /// sub-ranges of real files.
    fn real_path(&self) -> Option<&Path> {
        None
    }

    /// Describe where this Asset comes from.
    ///
    /// This should generally include the type of asset source (e.g. file vs mega file) and where
    /// that source is located; for a file asset that would be that path to the file, while for a
    /// mega file asset, that might be the path to the mega file.
    fn describe_source(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

/// Represents a file within the game, possibly independent, possibly packed in a MEGA file.
#[repr(transparent)]
pub struct Asset(dyn AssetKind);

impl Asset {
    /// Create a new asset from an inner asset kind.
    pub(crate) fn new<K: AssetKind>(inner: K) -> Box<Self> {
        let inner: Box<dyn AssetKind> = Box::new(inner);
        let ptr: *mut dyn AssetKind = Box::into_raw(inner);
        // Asset is repr(transparent) to dyn AssetKind, so we should be able to freely cast between
        // them.
        let ptr = ptr as *mut Asset;
        // SAFETY: since we got this pointer from a Box with the same repr, this should be safe.
        unsafe { Box::from_raw(ptr) }
    }

    /// Gets the relative path of the Asset within the game, as a case-insensitive path.
    pub fn path(&self) -> &MegPath {
        self.0.path()
    }

    /// Opens a new reader which can be used to read the content of this asset.
    pub fn open_reader(&self) -> io::Result<AssetReader> {
        self.0.open_reader().map(AssetReader)
    }

    /// Get the last modified time of this asset. May be used to skip copying or processing the
    /// asset if the destination is newer.
    pub fn modified_time(&self) -> io::Result<SystemTime> {
        self.0.modified_time()
    }

    /// If this Asset describes an on-disk file, this is the path to that file. None for file
    /// sources that have some transformation applied, such as MEGA file contents, which are
    /// sub-ranges of real files.
    pub fn real_path(&self) -> Option<&Path> {
        self.0.real_path()
    }

    /// Gets a value which can be formatted with 'display' to describe the source of this asset
    /// (e.g. whether it comes from a file or mega file).
    pub fn describe_source(&self) -> SourceDescription<'_> {
        SourceDescription(&self.0)
    }
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{} from {}", self.path(), self.describe_source())
        } else {
            write!(f, "{}", self.path())
        }
    }
}

/// A reader for an Asset.
pub struct AssetReader(Box<dyn Read>);

impl Read for AssetReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.read_vectored(bufs)
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.0.read_to_end(buf)
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.0.read_to_string(buf)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.0.read_exact(buf)
    }
}

/// When formatted as display, prints the source description of an asset.
#[derive(Copy, Clone)]
pub struct SourceDescription<'a>(&'a dyn AssetKind);

impl<'a> fmt::Display for SourceDescription<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.describe_source(f)
    }
}
