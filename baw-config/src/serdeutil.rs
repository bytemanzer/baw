/// Utilities for serializing configured scale.
pub mod scale {
    /// Gets the default value for the scale field.
    pub const fn default() -> f32 {
        1.0
    }

    /// Gets the default value of the
    pub const fn is_default(val: &f32) -> bool {
        *val == default()
    }
}
