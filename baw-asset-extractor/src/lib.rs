use std::io::{self, Read};
use std::path::Path;

pub mod file;
pub mod meg;
pub mod models;
pub mod textures;

/// Represents a file within the game, possibly independent, possibly packed in a MEGA file.
pub trait Asset {
    /// Gets the relative path of the Asset within the game.
    fn path(&self) -> &Path;

    /// Type of reader used by this asset. The reader is generally not buffered.
    type Reader<'r>: Read + 'r
    where
        Self: 'r;

    /// Opens a new reader which can be used to read the content of this asset.
    fn open_reader(&mut self) -> io::Result<Self::Reader<'_>>;
}

/// Finds assets and applies a visitor to them.
pub trait AssetFinder {
    /// Find all assets and pass them to the given visitor.
    fn find_assets<V: AssetVisitor>(&self, visitor: &mut V) -> anyhow::Result<()>;

    fn chain<F: AssetFinder>(self, other: F) -> Chain<Self, F>
    where
        Self: Sized,
    {
        Chain {
            finder1: self,
            finder2: other,
        }
    }
}

/// Chains two AssetFinders, finding all assets with one then the other.
pub struct Chain<F1, F2> {
    finder1: F1,
    finder2: F2,
}

impl<F1: AssetFinder, F2: AssetFinder> AssetFinder for Chain<F1, F2> {
    fn find_assets<V: AssetVisitor>(&self, visitor: &mut V) -> anyhow::Result<()> {
        self.finder1.find_assets(visitor)?;
        self.finder2.find_assets(visitor)?;
        Ok(())
    }
}

/// Visits assets one at a time, peforming operations on them as needed.
pub trait AssetVisitor {
    /// Visit an asset performing some operation on it.
    fn visit_asset<A: Asset>(&mut self, asset: &mut A) -> anyhow::Result<()>;
}

macro_rules! tuple_visitors {
    () => { };
    ($firstgeneric:ident $($restgeneric:ident)*) => {
        impl<$firstgeneric: AssetVisitor, $($restgeneric: AssetVisitor,)*> AssetVisitor for ($firstgeneric, $($restgeneric,)*) {
            fn visit_asset<Asst: Asset>(&mut self, asset: &mut Asst) -> anyhow::Result<()> {
                #[allow(non_snake_case)]
                let ($firstgeneric, $($restgeneric,)*) = self;
                AssetVisitor::visit_asset($firstgeneric, asset)?;
                $(AssetVisitor::visit_asset($restgeneric, asset)?;)*
                Ok(())
            }
        }
        tuple_visitors!($($restgeneric)*);
    };
}

tuple_visitors!(V1 V2 V3 V4 V5 V6 V7 V8 V9 V10 V11 V12 V13 V14 V15 V16 V17 V18 V19 V20);

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
