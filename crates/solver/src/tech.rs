// Technology model + registry: explains gaps, never gates selection.
use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// One Factorio technology: what it needs, what recipe access it grants.
///
/// This graph exists for explanation only — it turns "you cannot build this"
/// into "this needs `foundry`". Nothing in the solver selects a recipe, a
/// machine, or a layout based on `Technology`: `chain::solve` resolves from
/// `ChainGoal.available` and `ChainGoal.availability` — a recipe-name set,
/// never a technology set — and `Recipe::enabled` is informational rather
/// than a filter (research-locked recipes are legitimate goals).
/// `chain::select` does consult this graph, but only to *explain* a refusal —
/// `NotUnlocked`/`MachineNotUnlocked` name the technology to research —
/// while selection itself still keys on the recipe set: the graph explains,
/// it never gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Technology {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Technology names that must be researched first, in the dump's own
    /// order (see the ingest side for why it is not sorted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prerequisites: Vec<String>,
    /// Recipe names from `effects[]` entries of type `unlock-recipe`.
    ///
    /// Often empty: 112 of the real game's 275 technologies grant no recipe
    /// at all (pure damage/speed/productivity bonuses) and are still
    /// ingested, because a prerequisite chain can run straight through a
    /// bonus-only node on its way to one that does unlock something.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unlocks: Vec<String>,
}

// ── OnceLock registry ─────────────────────────────────────────────────

static REGISTRY: OnceLock<HashMap<String, Technology>> = OnceLock::new();

fn load_registry() -> HashMap<String, Technology> {
    let json = include_str!("../data/technologies.json");
    let technologies: Vec<Technology> = serde_json::from_str(json)
        .expect("technologies.json must be valid JSON matching the Technology schema");
    technologies.into_iter().map(|t| (t.name.clone(), t)).collect()
}

/// The full technology registry, loaded once from the committed data file.
pub fn registry() -> &'static HashMap<String, Technology> {
    REGISTRY.get_or_init(load_registry)
}

/// Lookup a technology by internal name. Returns None for unknown/modded names.
pub fn get(name: &str) -> Option<&'static Technology> {
    registry().get(name)
}

// ── Reverse index: recipe name -> technologies that unlock it ──────────

static UNLOCKERS: OnceLock<HashMap<String, Vec<&'static str>>> = OnceLock::new();

fn load_unlockers() -> HashMap<String, Vec<&'static str>> {
    let mut index: HashMap<String, Vec<&'static str>> = HashMap::new();
    for tech in registry().values() {
        for recipe in &tech.unlocks {
            index.entry(recipe.clone()).or_default().push(tech.name.as_str());
        }
    }
    for names in index.values_mut() {
        names.sort_unstable();
    }
    index
}

/// Names of technologies whose `unlocks` contain `recipe`, sorted.
///
/// Backed by a reverse index built once beside the registry: this is called
/// per error message, never per candidate in a hot search, but a linear scan
/// of ~275 technologies on every call would be pointless when the index is
/// free to build once. Unknown recipe -> empty vec, never a panic.
pub fn unlockers_for(recipe: &str) -> Vec<&'static str> {
    UNLOCKERS.get_or_init(load_unlockers).get(recipe).cloned().unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_loads_and_indexes_unlockers() {
        let _ = registry();
        assert!(unlockers_for("no-such-recipe").is_empty());
    }

    #[test]
    fn get_returns_none_for_an_unknown_name() {
        assert!(get("not-a-real-technology").is_none());
    }

    #[test]
    fn every_entry_is_keyed_by_its_own_name() {
        for (key, tech) in registry() {
            assert_eq!(key, &tech.name);
        }
    }

    #[test]
    fn round_trips_through_serde() {
        let original = Technology {
            name: "foundry".to_string(),
            display_name: Some("Foundry".to_string()),
            prerequisites: vec!["calcite-processing".to_string(), "tungsten-carbide".to_string()],
            unlocks: vec!["foundry".to_string(), "casting-iron".to_string()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: Technology = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn absent_optional_fields_default_and_are_omitted_from_output() {
        let bare: Technology = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
        assert_eq!(bare.display_name, None);
        assert!(bare.prerequisites.is_empty());
        assert!(bare.unlocks.is_empty());

        let json = serde_json::to_string(&bare).unwrap();
        assert!(!json.contains("display_name"));
        assert!(!json.contains("prerequisites"));
        assert!(!json.contains("unlocks"));
    }

    #[test]
    fn unlockers_for_a_recipe_the_placeholder_unlocks_is_non_empty_and_sorted() {
        // Derive the recipe from the registry itself rather than hardcoding
        // one, so this survives the orchestrator's regeneration from the
        // real dump (the committed placeholder is deliberately minimal).
        let recipe = registry()
            .values()
            .flat_map(|t| t.unlocks.iter())
            .next()
            .cloned()
            .expect("placeholder registry must unlock at least one recipe");

        let unlockers = unlockers_for(&recipe);
        assert!(!unlockers.is_empty());
        let mut sorted = unlockers.clone();
        sorted.sort_unstable();
        assert_eq!(unlockers, sorted);
    }
}
