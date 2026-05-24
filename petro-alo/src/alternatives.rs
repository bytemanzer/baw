use std::sync::LazyLock;

use regex::Regex;

/// Represents a LOD level of a mesh, bone, or proxy. Used to show different meshes depending on how
/// close an object is to the camera. Higher numbers are more detailed.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Lod(pub u32);

impl Lod {
    /// Extract the LOD level from a name.
    pub fn from_name(name: &str) -> Option<Lod> {
        static LOD_FINDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"LOD([0-9]+)").unwrap());
        parse_cap(&LOD_FINDER, name).map(Lod)
    }

    /// Extract the LOD group from the name. This removes both _LOD and _ALT if present.
    /// This is used in the process of assigning LodGroup ids.
    pub(crate) fn lod_group_name(name: &str) -> &str {
        // Find the name up to the *first* (non-greedy .*) _LOD or _ALT.
        static SUFFIX_STRIPPER: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^(.*?)_(LOD|ALT)").unwrap());
        match SUFFIX_STRIPPER.captures(name) {
            Some(caps) => caps.get(1).unwrap().as_str(),
            None => name,
        }
    }
}

/// An arbitrary opaque ID which identifies which Meshes in an ALO model are alternate LODs of the
/// same thing. This can be used to identify related LODs without needing to perform further name
/// inspection.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LodGroup(pub(crate) usize);

/// Represents an ALT level of a mesh, bone, or proxy. Used to hide or show different parts
/// depending on how damaged an object is. Higher numbers are more damaged.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Alt(pub u32);

impl Alt {
    pub fn from_name(name: &str) -> Option<Alt> {
        static ALT_FINDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"ALT([0-9]+)").unwrap());
        parse_cap(&ALT_FINDER, name).map(Alt)
    }
}

fn parse_cap(regex: &Regex, name: &str) -> Option<u32> {
    let val = match regex.captures(name) {
        None => return None,
        // This unwrap should always succeed if the regex matched because the group is not optional.
        Some(caps) => caps.get(1).unwrap().as_str(),
    };
    // Because the regex group matches only integer digits, the only way this could fail should be
    // if the matched number was so long it overflowed u32.
    val.parse::<u32>().ok()
}
