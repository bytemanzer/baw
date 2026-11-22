use std::any::type_name;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

use anyhow::bail;
use serde::Deserialize;
use tracing::warn;

use crate::xml::{MegPathXml, OptLast, TaggedXml, XmlF32, XmlIdent};

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
    for entry in values.iter() {
        if !all.insert(entry.name().to_string()) {
            bail!(
                "{} {:?} defined more than once",
                type_name::<V>(),
                entry.name()
            );
        }
    }
    for entry in values {
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
                entry.apply_default_parent();
                roots.insert(entry.name(), entry);
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
    pub files: Vec<MegPathXml>,
}

/// Deserializer type for any/all of the XML files listed in GameObjectFiles.XML.
#[derive(Deserialize, Debug, Default)]
pub struct GameObjectListXml {
    /// List of SpaceUnits defined in this game object file.
    #[serde(rename = "$value", default)]
    pub game_objects: Vec<TaggedXml<GameObjectXml>>,
}

#[derive(Deserialize, Debug)]
pub struct GameObjectXml {
    /// The collection this game object belongs to. This comes from the XML tag of the outer list
    /// tag on the game object list. In the following example, the collection is
    /// `SpaceUnitsCapital`:
    ///
    /// ```xml
    /// <SpaceUnitsCapital>
    ///     <SpaceUnit>
    ///         ...
    /// ```
    ///
    /// The collection is `skip` for serde, because we need to capture it with [`TaggedXml`] and
    /// fill it later.
    #[serde(skip)]
    pub collection: Arc<str>,
    /// The kind of game object this is. This comes from the XML tag of the element within the game
    /// object list. In the following example, the category is `SpaceUnit`:
    ///
    /// ```xml
    /// <SpaceUnitsCapital>
    ///     <SpaceUnit>
    ///         ...
    /// ```
    ///
    /// The kind is `skip` for serde, because we need to capture it with [`TaggedXml`] and fill it
    /// later.
    #[serde(skip)]
    pub kind: String,
    /// Name of this game object. This serves as a unique key that can be used to reference the game
    /// object later from other configs.
    #[serde(rename = "@Name")]
    pub name: XmlIdent,
    /// Name of another game object that this one inherits configuration from.
    #[serde(rename = "Variant_Of_Existing_Type", default)]
    pub variant_of_existing_type: OptLast<XmlIdent>,
    /// Name of the model to use for this game object in space battles, if any.
    // TODO: figure out how to handle space vs land model for BAW since we won't have quite as clean
    // of a land/space split as old EaW did.
    #[serde(rename = "Space_Model_Name", default)]
    pub space_model_name: OptLast<MegPathXml>,
    /// Scale to apply to the model.
    #[serde(rename = "Scale_Factor", default)]
    pub scale_factor: OptLast<XmlF32>,
}

impl VariantOfExisting for GameObjectXml {
    fn name(&self) -> &str {
        &self.name
    }

    fn variant_of_existing_type(&self) -> Option<&str> {
        self.variant_of_existing_type.as_str()
    }

    fn apply_parent(&mut self, parent: &Self) {
        self.variant_of_existing_type.0 = None;
        apply_parent!(self, parent, [space_model_name, scale_factor]);
    }

    fn apply_default_parent(&mut self) {
        self.variant_of_existing_type.0 = None;
    }
}
