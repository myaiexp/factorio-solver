/// Broad category for a Factorio entity.
///
/// This enum provides the canonical classification logic used throughout the crate
/// for ASCII rendering and GUI coloring. Classification order is critical: more
/// specific patterns (e.g. "underground") must be tested before general ones (e.g. "belt").
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
    /// The matching order is critical: more specific patterns (e.g. "underground")
    /// are tested before general ones (e.g. "belt") to ensure correct classification.
    ///
    /// # Examples
    ///
    /// ```
    /// use factorio_grid::EntityCategory;
    ///
    /// assert_eq!(
    ///     EntityCategory::from_prototype_name("transport-belt"),
    ///     EntityCategory::Belt
    /// );
    /// assert_eq!(
    ///     EntityCategory::from_prototype_name("underground-belt"),
    ///     EntityCategory::UndergroundBelt
    /// );
    /// ```
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

    /// Single character for ASCII rendering.
    ///
    /// B=belt, I=inserter, A=assembler, F=furnace, S=splitter,
    /// U=underground belt, P=pipe, E=electric pole, C=chemical plant,
    /// R=refinery, K=beacon, X=combinator, L=lamp, ?=unknown
    ///
    /// # Examples
    ///
    /// ```
    /// use factorio_grid::EntityCategory;
    ///
    /// let category = EntityCategory::from_prototype_name("transport-belt");
    /// assert_eq!(category.label_char(), 'B');
    /// ```
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

    // ── Classification tests ─────────────────────────────────────────────────

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

    // ── Belt variants ────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_belts() {
        assert_eq!(
            EntityCategory::from_prototype_name("transport-belt").label_char(),
            'B'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-transport-belt").label_char(),
            'B'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("express-transport-belt").label_char(),
            'B'
        );
    }

    #[test]
    fn test_char_mapping_underground() {
        assert_eq!(
            EntityCategory::from_prototype_name("underground-belt").label_char(),
            'U'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-underground-belt").label_char(),
            'U'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("express-underground-belt").label_char(),
            'U'
        );
    }

    #[test]
    fn test_char_mapping_splitters() {
        assert_eq!(
            EntityCategory::from_prototype_name("splitter").label_char(),
            'S'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-splitter").label_char(),
            'S'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("express-splitter").label_char(),
            'S'
        );
    }

    // ── Inserters ────────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_inserters() {
        assert_eq!(
            EntityCategory::from_prototype_name("inserter").label_char(),
            'I'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("fast-inserter").label_char(),
            'I'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("long-handed-inserter").label_char(),
            'I'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("stack-inserter").label_char(),
            'I'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("bulk-inserter").label_char(),
            'I'
        );
    }

    // ── Assemblers ───────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_assemblers() {
        assert_eq!(
            EntityCategory::from_prototype_name("assembling-machine-1").label_char(),
            'A'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("assembling-machine-2").label_char(),
            'A'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("assembling-machine-3").label_char(),
            'A'
        );
    }

    // ── Furnaces ─────────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_furnaces() {
        assert_eq!(
            EntityCategory::from_prototype_name("stone-furnace").label_char(),
            'F'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("steel-furnace").label_char(),
            'F'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("electric-furnace").label_char(),
            'F'
        );
    }

    // ── Chemical & Refinery ──────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_chemical_and_refinery() {
        assert_eq!(
            EntityCategory::from_prototype_name("chemical-plant").label_char(),
            'C'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("oil-refinery").label_char(),
            'R'
        );
    }

    // ── Pipes ────────────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_pipes() {
        assert_eq!(
            EntityCategory::from_prototype_name("pipe").label_char(),
            'P'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("pipe-to-ground").label_char(),
            'P'
        );
    }

    // ── Electric poles ───────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_electric_poles() {
        assert_eq!(
            EntityCategory::from_prototype_name("small-electric-pole").label_char(),
            'E'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("medium-electric-pole").label_char(),
            'E'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("big-electric-pole").label_char(),
            'E'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("substation").label_char(),
            'E'
        );
    }

    // ── Miscellaneous ────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_misc() {
        assert_eq!(
            EntityCategory::from_prototype_name("beacon").label_char(),
            'K'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("small-lamp").label_char(),
            'L'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("arithmetic-combinator").label_char(),
            'X'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("decider-combinator").label_char(),
            'X'
        );
        assert_eq!(
            EntityCategory::from_prototype_name("constant-combinator").label_char(),
            'X'
        );
    }

    // ── Unknown ──────────────────────────────────────────────────────────────

    #[test]
    fn test_char_mapping_unknown() {
        assert_eq!(
            EntityCategory::from_prototype_name("something-weird").label_char(),
            '?'
        );
    }

    // ── Label chars uniqueness ───────────────────────────────────────────────

    #[test]
    fn test_distinct_label_chars() {
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
        let chars: Vec<_> = categories.iter().map(|c| c.label_char()).collect();
        for i in 0..chars.len() {
            for j in (i + 1)..chars.len() {
                assert_ne!(
                    chars[i], chars[j],
                    "{:?} and {:?} share a label char",
                    categories[i], categories[j]
                );
            }
        }
    }
}
