use petro_meg::path::MegPathBuf;
use serde::Deserialize;

use crate::xml;

/// Deserializer type for an XML file containing a list of `<File>` elements specifying other files
/// to read.
#[derive(Deserialize, Debug)]
pub struct FileListXml {
    /// List of files referenced by this file.
    #[serde(rename = "File")]
    #[serde(deserialize_with = "xml::deserialize_asset_paths")]
    pub files: Vec<MegPathBuf>,
}

/// Deserializer type for any/all of the XML files listed in GameObjectFiles.XML.
#[derive(Deserialize, Debug)]
pub struct GameObjectListXml {
    /// List of SpaceUnits defined in this game object file.
    #[serde(rename = "SpaceUnit", default)]
    pub spaceunits: Vec<SpaceUnitXml>,
}

#[derive(Deserialize, Debug)]
pub struct SpaceUnitXml {
    /// Name of this space unit.
    #[serde(rename = "@Name")]
    pub name: String,
    /// Name of the model to use for this ship.
    #[serde(
        rename = "Space_Model_Name",
        deserialize_with = "xml::deserialize_opt_asset_path",
        default
    )]
    pub space_model_name: Option<MegPathBuf>,
    /// Name of another unit that this one inherits from.
    #[serde(rename = "Variant_Of_Existing_Type")]
    pub variant_of_existing_type: Option<String>,
}
