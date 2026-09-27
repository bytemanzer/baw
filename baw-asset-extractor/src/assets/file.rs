use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Context as _;
use petro_meg::path::{MegPath, MegPathBuf};
use tracing::debug;
use walkdir::WalkDir;

use crate::assets::Asset;
use crate::assets::asset::AssetKind;

/// An asset representing a file on disk.
struct FileAsset {
    /// Path to the asset file on disk.
    real_path: PathBuf,
    /// Path name used to refer to the file.
    nominal_path: MegPathBuf,
}

impl AssetKind for FileAsset {
    fn path(&self) -> &MegPath {
        &self.nominal_path
    }

    fn open_reader(&self) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(File::open(&self.real_path)?))
    }

    fn modified_time(&self) -> io::Result<SystemTime> {
        std::fs::metadata(&self.real_path)?.modified()
    }

    fn real_path(&self) -> Option<&Path> {
        Some(&self.real_path)
    }

    fn describe_source(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<FileAsset at {}>", self.real_path.display())
    }
}

pub fn find_asset_files(
    game_data: impl Into<Arc<Path>>,
) -> impl Iterator<Item = anyhow::Result<Box<Asset>>> {
    let game_data = game_data.into();
    WalkDir::new(&game_data)
        .follow_links(true)
        .into_iter()
        .filter_entry({
            let game_data = game_data.clone();
            move |entry| {
                let path = entry.path().strip_prefix(&game_data).unwrap();
                // We have to allow an empty prefix, otherwise we'll filter the entire GameData
                // directory!
                if path.is_empty() {
                    return true;
                }
                path_starts_with_ignore_case(path, "Data")
            }
        })
        .map(move |entry| -> anyhow::Result<Option<Box<Asset>>> {
            let entry = entry?;
            let canonical_path = match std::fs::canonicalize(entry.path()) {
                Ok(canonical) => canonical,
                // If the ultimate canonical referent of the path is not found, it may be a dead
                // symlink. We'll just ignore it.
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    debug!(
                        "skipping {} because the canonical path was not found",
                        entry.path().display()
                    );
                    return Ok(None);
                }
                Err(err) => Err(err).context(format!(
                    "failed to resolve the canonical path to {}",
                    entry.path().display()
                ))?,
            };
            match std::fs::metadata(canonical_path) {
                Ok(meta) => {
                    if !meta.file_type().is_file() {
                        debug!(
                            "skipping {} because it is not a file",
                            entry.path().display()
                        );
                        return Ok(None);
                    }
                }
                // If the ultimate canonical referent of the path is not found, it may be a dead
                // symlink. We'll just ignore it.
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    debug!(
                        "skipping {} because the canonical path was not found",
                        entry.path().display()
                    );
                    return Ok(None);
                }
                Err(err) => Err(err).context(format!(
                    "failed to resolve the canonical path to {}",
                    entry.path().display()
                ))?,
            }
            let real_path = entry.path().to_path_buf();
            let relative_path = real_path.strip_prefix(&game_data).unwrap();
            let nominal_path =
                MegPath::from_bytes(relative_path.as_os_str().as_encoded_bytes())?.to_owned();
            Ok(Some(Asset::new(FileAsset {
                real_path,
                nominal_path,
            })))
        })
        .filter_map(|entry| entry.transpose())
}

fn path_starts_with_ignore_case(path: impl AsRef<Path>, prefix: impl AsRef<Path>) -> bool {
    let mut path = path.as_ref().components();
    let mut prefix = prefix.as_ref().components();
    loop {
        match (path.next(), prefix.next()) {
            (Some(_), None) | (None, None) => return true,
            (None, Some(_)) => return false,
            (Some(path_component), Some(prefix_component)) => {
                if !path_component
                    .as_os_str()
                    .eq_ignore_ascii_case(prefix_component.as_os_str())
                {
                    return false;
                }
            }
        }
    }
}
