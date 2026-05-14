use std::fmt;

/// Label for sub-assets of an ALO model.
#[derive(Clone, Debug)]
pub enum AloAssetLabel {
    Submesh { mesh_idx: usize, submesh_idx: usize },
    SubmeshMaterial { mesh_idx: usize, submesh_idx: usize },
    SubmeshInverseBindposes { mesh_idx: usize, submesh_idx: usize },
}

impl fmt::Display for AloAssetLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Submesh {
                mesh_idx,
                submesh_idx,
            } => {
                write!(f, "Mesh{mesh_idx}Submesh{submesh_idx}")?;
            }
            Self::SubmeshMaterial {
                mesh_idx,
                submesh_idx,
            } => {
                write!(f, "Mesh{mesh_idx}Submesh{submesh_idx}Material")?;
            }
            Self::SubmeshInverseBindposes {
                mesh_idx,
                submesh_idx,
            } => {
                write!(f, "Mesh{mesh_idx}Submesh{submesh_idx}InverseBindposes")?;
            }
        }
        Ok(())
    }
}
