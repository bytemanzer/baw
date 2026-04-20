use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail};
use petro_meg::path::MegPath;
use petro_meg::reader::{FileEntry, FileSegmentReader, MegReadOptions, ReadMegMeta};
use petro_meg::version::MegV1;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use tracing::{debug, trace, warn};
use walkdir::WalkDir;

use crate::{Asset, AssetFinder, AssetVisitor, path_starts_with_ignore_case};

/// An asset backed by a MEGA file entry.
struct MegAsset<'r, R> {
    entry: FileEntry,
    path: PathBuf,
    reader: &'r mut R,
}

impl<'r, R: Read + Seek> Asset for MegAsset<'r, R> {
    fn path(&self) -> &Path {
        &self.path
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

pub struct MegAssetFinder<'p> {
    game_data: &'p Path,
}

impl<'p> MegAssetFinder<'p> {
    /// Make a megafile asset finder that searches the given directory.
    pub fn new(game_data: &'p Path) -> Self {
        Self { game_data }
    }

    /// Walks the game data directory finding all files. Returns a mapping from the lower-cased file
    /// name, relative to the game data directory, to the full path. This helper is intended to
    /// allow case-insensitive file names to work on systems with case-sensitive filesystems.
    fn find_lowercase_files(&self) -> anyhow::Result<HashMap<PathBuf, PathBuf>> {
        let mut lower_files = HashMap::new();

        let walker = WalkDir::new(self.game_data)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                if entry.path() == self.game_data {
                    return true;
                }
                let path = entry.path().strip_prefix(self.game_data).unwrap();
                path_starts_with_ignore_case(path, "Data")
            });
        for entry in walker {
            let entry = entry?;
            let mut lower_relative_path = entry
                .path()
                .strip_prefix(self.game_data)
                .unwrap()
                .to_path_buf();
            lower_relative_path.as_mut_os_str().make_ascii_lowercase();
            trace!("Mapping from {lower_relative_path:?} to {:?}", entry.path());
            lower_files.insert(lower_relative_path, entry.path().to_path_buf());
        }
        Ok(lower_files)
    }

    /// Finds the megafiles.xml file and returns a list of the real paths to all mega files listed
    /// in the megafiles.xml which actuall exist in the directory.
    fn list_mega_files<'f>(
        &self,
        files: &'f HashMap<PathBuf, PathBuf>,
    ) -> anyhow::Result<Vec<&'f Path>> {
        let megafiles = files
            .get(Path::new("data/megafiles.xml"))
            .ok_or_else(|| anyhow!("Data/megafiles.xml not found"))?;
        let megafiles = BufReader::new(File::open(megafiles)?);

        let mut result = Vec::new();

        let mut buf = Vec::new();
        let mut reader = Reader::from_reader(megafiles);
        reader.config_mut().trim_text(true);
        let mut path = Vec::<Vec<u8>>::new();
        loop {
            buf.clear();
            if path
                .iter()
                .map(|v| &**v)
                .eq([b"Mega_Files".as_slice(), b"File"])
            {
                let text =
                    reader.read_text_into(QName(path.last().unwrap().as_slice()), &mut buf)?;
                path.pop();
                let text = text.decode()?;
                let file = MegPath::from_str(text.trim())?;
                let mut file = file.to_path_buf();
                file.as_mut_os_str().make_ascii_lowercase();
                if let Some(realpath) = files.get(&file) {
                    debug!("Path {text:?} found at {realpath:?}");
                    result.push(&**realpath);
                } else {
                    warn!("Mega file path {file:?} did not correspond to a real file");
                }
            } else {
                match reader.read_event_into(&mut buf)? {
                    Event::Start(start) => {
                        path.push(start.name().0.to_owned());
                    }
                    Event::End(end) => {
                        if Some(end.name().0) != path.last().map(|v| &**v) {
                            bail!(
                                "Found end tag {:?} but current path is {:?}",
                                end.name(),
                                path.last().map(|v| QName(&**v))
                            );
                        }
                        path.pop();
                    }
                    Event::Eof => break,
                    // We ignore all other event types.
                    _ => {}
                }
            }
        }

        Ok(result)
    }
}

impl<'p> AssetFinder for MegAssetFinder<'p> {
    fn find_assets<V: AssetVisitor>(&self, visitor: &mut V) -> anyhow::Result<()> {
        let files = self.find_lowercase_files()?;
        let megafiles = self.list_mega_files(&files)?;
        for file in megafiles {
            let mut file = File::open(file)?;
            let subfiles = MegV1.read_meg_meta(BufReader::new(&mut file))?;
            for subfile in subfiles {
                let subfile_path = subfile.name().to_path_buf();
                let mut asset = MegAsset {
                    entry: subfile,
                    path: subfile_path,
                    reader: &mut file,
                };
                visitor.visit_asset(&mut asset)?;
            }
        }

        Ok(())
    }
}
