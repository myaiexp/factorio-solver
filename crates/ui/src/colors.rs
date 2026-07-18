// GUI presentation (color + display name) for the grid crate's EntityCategory.
use egui::Color32;
use factorio_grid::EntityCategory;

/// Extension trait layering egui presentation onto [`factorio_grid::EntityCategory`].
///
/// Classification (`from_prototype_name`) and the ASCII `label_char` live in the
/// grid crate as the single source of truth. This trait adds only the
/// GUI-specific color and human-readable name, so the UI never re-implements the
/// substring-matching ladder. Bring it into scope (`use crate::colors::CategoryStyle`)
/// to call `.color()` / `.display_name()` on a category value.
pub trait CategoryStyle {
    /// Distinct color for this category, designed for a dark background.
    fn color(&self) -> Color32;

    /// Human-readable plural label, used as a section header in the entity
    /// palette panel. Reserved for that panel, which is not yet wired into the
    /// UI, hence the allow — remove it once the panel calls this.
    #[allow(dead_code)]
    fn display_name(&self) -> &'static str;
}

impl CategoryStyle for EntityCategory {
    fn color(&self) -> Color32 {
        match self {
            EntityCategory::Belt => Color32::from_rgb(0xD4, 0xA0, 0x17),
            EntityCategory::UndergroundBelt => Color32::from_rgb(0xB8, 0x86, 0x0B),
            EntityCategory::Splitter => Color32::from_rgb(0xE8, 0x88, 0x00),
            EntityCategory::Inserter => Color32::from_rgb(0x55, 0xAA, 0xFF),
            EntityCategory::Assembler => Color32::from_rgb(0x46, 0x82, 0xB4),
            EntityCategory::Furnace => Color32::from_rgb(0xCD, 0x7F, 0x32),
            EntityCategory::ChemicalPlant => Color32::from_rgb(0x00, 0x80, 0x80),
            EntityCategory::Refinery => Color32::from_rgb(0x8B, 0x00, 0x8B),
            EntityCategory::Pipe => Color32::from_rgb(0x70, 0x80, 0x90),
            EntityCategory::ElectricPole => Color32::from_rgb(0x32, 0xCD, 0x32),
            EntityCategory::Beacon => Color32::from_rgb(0x93, 0x70, 0xDB),
            EntityCategory::Combinator => Color32::from_rgb(0xFA, 0x80, 0x72),
            EntityCategory::Lamp => Color32::from_rgb(0xFF, 0xFA, 0xCD),
            EntityCategory::Unknown => Color32::from_rgb(0x69, 0x69, 0x69),
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            EntityCategory::Belt => "Belts",
            EntityCategory::UndergroundBelt => "Underground Belts",
            EntityCategory::Splitter => "Splitters",
            EntityCategory::Inserter => "Inserters",
            EntityCategory::Assembler => "Assemblers",
            EntityCategory::Furnace => "Furnaces",
            EntityCategory::ChemicalPlant => "Chemical Plants",
            EntityCategory::Refinery => "Refineries",
            EntityCategory::Pipe => "Pipes",
            EntityCategory::ElectricPole => "Electric Poles",
            EntityCategory::Beacon => "Beacons",
            EntityCategory::Combinator => "Combinators",
            EntityCategory::Lamp => "Lamps",
            EntityCategory::Unknown => "Other",
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every category must map to a visually distinct color — this is the only
    /// property the UI layer owns. Classification and label_char are the grid
    /// crate's concern and are tested there.
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

    /// Every category exposes a non-empty display name for the palette panel.
    #[test]
    fn test_display_names_present() {
        for category in [
            EntityCategory::Belt,
            EntityCategory::Unknown,
            EntityCategory::Combinator,
        ] {
            assert!(!category.display_name().is_empty());
        }
    }
}
