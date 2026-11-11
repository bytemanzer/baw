use std::any::type_name;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use anyhow::bail;
use petro_meg::path::MegPathBuf;
use serde::Deserialize;
use tracing::warn;

use crate::xml;

macro_rules! apply_parent {
    ($self:ident, $parent:ident, [$($field:ident),* $(,)?]) => {
        $(
            if $self.$field.is_none() {
                $self.$field = $parent.$field.clone();
            }
        )*
    };
}

/// Trait for XML configs that can describe themselves as variants of other configs.
pub trait VariantOfExisting {
    /// Gets the name of this variant.
    fn name(&self) -> &str;

    /// If this config is a variant of an existing type, gets the type it is derived from.
    fn variant_of_existing_type(&self) -> Option<&str>;

    /// Updates self with corresponding fields from the parent.
    ///
    /// This should also clear variant_of_existing_type.
    fn apply_parent(&mut self, parent: &Self);

    /// Updates self with corresponding default fields, as the parent was not found.
    ///
    /// This should also clear variant_of_existing_type.
    fn apply_default_parent(&mut self);
}

/// Applies all parents for the given values and ensures values have unique names.
pub fn apply_parents<V: VariantOfExisting>(values: &mut [V]) -> anyhow::Result<()> {
    let mut all = HashSet::new();
    let mut roots: HashMap<&str, &V> = HashMap::new();
    let mut remaining = VecDeque::new();
    for entry in values {
        if !all.insert(entry.name().to_string()) {
            bail!(
                "{} {:?} defined more than once",
                type_name::<V>(),
                entry.name()
            );
        }
        if let Some(parent) = entry.variant_of_existing_type() {
            if all.contains(parent) {
                remaining.push_back(entry);
            } else {
                warn!(
                    "{} {:?} uses parent {:?} which does not exist",
                    type_name::<V>(),
                    entry.name(),
                    parent
                );
            }
        } else {
            roots.insert(entry.name(), entry);
        }
    }
    loop {
        let mut num_rooted = 0usize;
        for _ in 0..remaining.len() {
            let next = remaining.pop_front().unwrap();
            let parent = match roots.get(next.variant_of_existing_type().unwrap()) {
                None => {
                    remaining.push_back(next);
                    continue;
                }
                Some(parent) => *parent,
            };
            next.apply_parent(parent);
            roots.insert(next.name(), next);
            num_rooted += 1;
        }
        if remaining.is_empty() {
            break;
        }
        if num_rooted == 0 {
            struct Circ<'a, V>(VecDeque<&'a mut V>);
            impl<'a, V: VariantOfExisting> fmt::Display for Circ<'a, V> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    for entry in self.0.iter() {
                        writeln!(
                            f,
                            "    {:?} -> {:?}",
                            entry.name(),
                            entry.variant_of_existing_type().unwrap()
                        )?;
                    }
                    Ok(())
                }
            }

            bail!(
                "Detected {} {} variants with circular Variant_Of_Existing_Type references:\n{}",
                remaining.len(),
                type_name::<V>(),
                Circ(remaining)
            )
        }
    }
    Ok(())
}

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
#[derive(Deserialize, Debug, Default)]
pub struct GameObjectListXml {
    /// List of SpaceUnits defined in this game object file.
    #[serde(rename = "SpaceUnit", default)]
    pub spaceunits: Vec<SpaceUnitXml>,
}

impl GameObjectListXml {
    /// Merge the objects from another game object list into this one.
    pub fn merge(&mut self, other: GameObjectListXml) {
        self.spaceunits.extend(other.spaceunits)
    }

    /// Applies all `Variant_Of_Existing_Type` relationshps.
    pub fn apply_all_parents(&mut self) -> anyhow::Result<()> {
        apply_parents(&mut self.spaceunits)?;
        Ok(())
    }
}

impl Extend<GameObjectListXml> for GameObjectListXml {
    fn extend<T: IntoIterator<Item = GameObjectListXml>>(&mut self, iter: T) {
        for item in iter {
            self.merge(item);
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct SpaceUnitXml {
    /// Name of this space unit.
    #[serde(rename = "@Name")]
    pub name: String,
    /// Name of another unit that this one inherits from.
    #[serde(rename = "Variant_Of_Existing_Type")]
    pub variant_of_existing_type: Option<String>,
    /// Name of the model to use for this ship.
    #[serde(
        rename = "Space_Model_Name",
        deserialize_with = "xml::deserialize_opt_asset_path",
        default = "xml::none",
    )]
    pub space_model_name: Option<MegPathBuf>,
    /// Scale to apply to the model.
    #[serde(rename = "Scale_Factor")]
    pub scale_factor: Option<f32>,
}

impl VariantOfExisting for SpaceUnitXml {
    fn name(&self) -> &str {
        &self.name
    }

    fn variant_of_existing_type(&self) -> Option<&str> {
        self.variant_of_existing_type.as_deref().map(str::trim)
    }

    fn apply_parent(&mut self, parent: &Self) {
        self.variant_of_existing_type = None;
        apply_parent!(self, parent, [space_model_name]);
    }

    fn apply_default_parent(&mut self) {
        self.variant_of_existing_type = None;
    }
}
