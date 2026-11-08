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

#[cfg(test)]
mod tests;
