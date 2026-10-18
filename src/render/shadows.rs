//! Render policy for the public `GrassShadows` setting.

impl crate::GrassShadows {
    /// Directional cascades grass is drawn into, counted from the nearest.
    pub(crate) fn cascades(self) -> usize {
        match self {
            crate::GrassShadows::Off => 0,
            crate::GrassShadows::Near => 1,
            crate::GrassShadows::Full => 2,
        }
    }

    /// Buckets drawn into shadow maps, counted from LOD 0. Orthographic views
    /// put every blade in bucket 0, so they always cast all of them.
    pub(crate) fn buckets(self) -> u64 {
        match self {
            crate::GrassShadows::Off | crate::GrassShadows::Near => 1,
            crate::GrassShadows::Full => 2,
        }
    }
}
