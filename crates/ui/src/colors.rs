use egui::Color32;

// Re-export EntityCategory from grid crate (canonical implementation)
pub use factorio_grid::EntityCategory;

/// Extension trait for EntityCategory providing GUI-specific color mapping.
pub trait EntityCategoryExt {
    /// Distinct color for this category, designed for a dark background.
    fn color(&self) -> Color32;
}

impl EntityCategoryExt for EntityCategory {
    fn color(&self) -> Color32 {
        match self {
            Self::Belt => Color32::from_rgb(0xD4, 0xA0, 0x17),
            Self::UndergroundBelt => Color32::from_rgb(0xB8, 0x86, 0x0B),
            Self::Splitter => Color32::from_rgb(0xE8, 0x88, 0x00),
            Self::Inserter => Color32::from_rgb(0x55, 0xAA, 0xFF),
            Self::Assembler => Color32::from_rgb(0x46, 0x82, 0xB4),
            Self::Furnace => Color32::from_rgb(0xCD, 0x7F, 0x32),
            Self::ChemicalPlant => Color32::from_rgb(0x00, 0x80, 0x80),
            Self::Refinery => Color32::from_rgb(0x8B, 0x00, 0x8B),
            Self::Pipe => Color32::from_rgb(0x70, 0x80, 0x90),
            Self::ElectricPole => Color32::from_rgb(0x32, 0xCD, 0x32),
            Self::Beacon => Color32::from_rgb(0x93, 0x70, 0xDB),
            Self::Combinator => Color32::from_rgb(0xFA, 0x80, 0x72),
            Self::Lamp => Color32::from_rgb(0xFF, 0xFA, 0xCD),
            Self::Unknown => Color32::from_rgb(0x69, 0x69, 0x69),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{EntityCategory, EntityCategoryExt};

    #[test]
    fn test_distinct_colors() {
        let categories = [
            EntityCategory::Belt,
            EntityCategory::UndergroundBelt,
            EntityCategory::Splitter,
            EntityCategory::Inserter,
            EntityCategory::Assembler,
            EntityCategory::Furnace,
            EntityCategory::ChemicalPlant,
            EntityCategory::Refinery,
            EntityCategory::Pipe,
            EntityCategory::ElectricPole,
            EntityCategory::Beacon,
            EntityCategory::Combinator,
            EntityCategory::Lamp,
            EntityCategory::Unknown,
        ];
        let colors: Vec<_> = categories.iter().map(|c| c.color()).collect();
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "{:?} and {:?} share a color",
                    categories[i], categories[j]
                );
            }
        }
    }

    #[test]
    fn test_label_chars_match_ascii_renderer() {
        assert_eq!(EntityCategory::Belt.label_char(), 'B');
        assert_eq!(EntityCategory::UndergroundBelt.label_char(), 'U');
        assert_eq!(EntityCategory::Splitter.label_char(), 'S');
        assert_eq!(EntityCategory::Inserter.label_char(), 'I');
        assert_eq!(EntityCategory::Assembler.label_char(), 'A');
        assert_eq!(EntityCategory::Furnace.label_char(), 'F');
        assert_eq!(EntityCategory::Unknown.label_char(), '?');
    }
}
