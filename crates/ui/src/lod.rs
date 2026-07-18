// Level-of-detail tiers that scale entity rendering to the current zoom.

/// How much per-entity detail the viewport draws, chosen from the zoom level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodLevel {
    /// Filled rect + border stroke + label character.
    Full,
    /// Filled rect only — stroke and text skipped.
    Medium,
    /// Single muted filled rect; at this scale detail is imperceptible.
    Minimal,
}

/// Zoom (pixels per cell) at or above which entities render at full detail.
const FULL_DETAIL_ZOOM: f32 = 16.0;
/// Zoom (pixels per cell) at or above which entities render at medium detail.
const MEDIUM_DETAIL_ZOOM: f32 = 4.0;

/// Map a zoom level (pixels per world cell) to its [`LodLevel`] tier.
pub fn lod_for_zoom(zoom: f32) -> LodLevel {
    if zoom >= FULL_DETAIL_ZOOM {
        LodLevel::Full
    } else if zoom >= MEDIUM_DETAIL_ZOOM {
        LodLevel::Medium
    } else {
        LodLevel::Minimal
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_tiers() {
        assert_eq!(lod_for_zoom(32.0), LodLevel::Full);
        assert_eq!(lod_for_zoom(16.0), LodLevel::Full); // inclusive boundary
        assert_eq!(lod_for_zoom(15.9), LodLevel::Medium);
        assert_eq!(lod_for_zoom(4.0), LodLevel::Medium); // inclusive boundary
        assert_eq!(lod_for_zoom(3.9), LodLevel::Minimal);
        assert_eq!(lod_for_zoom(1.0), LodLevel::Minimal);
    }
}
