use egui::Color32;

/// Broad category for a Factorio entity, used for coloring in the GUI.
///
/// Classification order mirrors `char_for_prototype` in `crates/grid/src/render.rs`
/// so the same substring-matching priority is preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityCategory {
    Belt,
    UndergroundBelt,
    Splitter,
    Inserter,
    Assembler,
    Furnace,
    ChemicalPlant,
    Refinery,
    Pipe,
    ElectricPole,
    Beacon,
    Combinator,
    Lamp,
    Unknown,
}

impl EntityCategory {
    /// Classify a prototype name into a category.
    ///
    /// The matching order is identical to `char_for_prototype` in
    /// `crates/grid/src/render.rs` — more specific patterns (e.g. "underground")
    /// are tested before general ones (e.g. "belt").
    pub fn from_prototype_name(name: &str) -> Self {
        if name.contains("underground") {
            Self::UndergroundBelt
        } else if name.contains("splitter") {
            Self::Splitter
        } else if name.contains("belt") {
            Self::Belt
        } else if name.contains("inserter") {
            Self::Inserter
        } else if name.contains("assembling") {
            Self::Assembler
        } else if name.contains("furnace") {
            Self::Furnace
        } else if name.contains("chemical") {
            Self::ChemicalPlant
        } else if name.contains("refinery") {
            Self::Refinery
        } else if name.contains("pipe") {
            Self::Pipe
        } else if name.contains("electric-pole") || name.contains("substation") {
            Self::ElectricPole
        } else if name.contains("beacon") {
            Self::Beacon
        } else if name.contains("combinator") {
            Self::Combinator
        } else if name.contains("lamp") {
            Self::Lamp
        } else {
            Self::Unknown
        }
    }

    /// Distinct color for this category, designed for a dark background.
    pub fn color(&self) -> Color32 {
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

    /// Single character matching the ASCII renderer in `crates/grid/src/render.rs`.
    pub fn label_char(&self) -> char {
        match self {
            Self::Belt => 'B',
            Self::UndergroundBelt => 'U',
            Self::Splitter => 'S',
            Self::Inserter => 'I',
            Self::Assembler => 'A',
            Self::Furnace => 'F',
            Self::ChemicalPlant => 'C',
            Self::Refinery => 'R',
            Self::Pipe => 'P',
            Self::ElectricPole => 'E',
            Self::Beacon => 'K',
            Self::Combinator => 'X',
            Self::Lamp => 'L',
            Self::Unknown => '?',
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_classification() {
        assert_eq!(
            EntityCategory::from_prototype_name("transport-belt"),
            EntityCategory::Belt
        );
        assert_eq!(
            EntityCategory::from_prototype_name("underground-belt"),
            EntityCategory::UndergroundBelt
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-splitter"),
            EntityCategory::Splitter
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-inserter"),
            EntityCategory::Inserter
        );
        assert_eq!(
            EntityCategory::from_prototype_name("assembling-machine-3"),
            EntityCategory::Assembler
        );
        assert_eq!(
            EntityCategory::from_prototype_name("electric-furnace"),
            EntityCategory::Furnace
        );
        assert_eq!(
            EntityCategory::from_prototype_name("chemical-plant"),
            EntityCategory::ChemicalPlant
        );
        assert_eq!(
            EntityCategory::from_prototype_name("oil-refinery"),
            EntityCategory::Refinery
        );
        assert_eq!(
            EntityCategory::from_prototype_name("pipe-to-ground"),
            EntityCategory::Pipe
        );
        assert_eq!(
            EntityCategory::from_prototype_name("substation"),
            EntityCategory::ElectricPole
        );
        assert_eq!(
            EntityCategory::from_prototype_name("beacon"),
            EntityCategory::Beacon
        );
        assert_eq!(
            EntityCategory::from_prototype_name("decider-combinator"),
            EntityCategory::Combinator
        );
        assert_eq!(
            EntityCategory::from_prototype_name("small-lamp"),
            EntityCategory::Lamp
        );
        assert_eq!(
            EntityCategory::from_prototype_name("something-modded"),
            EntityCategory::Unknown
        );
    }

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
