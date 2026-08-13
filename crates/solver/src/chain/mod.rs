// Production-chain calculator: a ChainGoal in, a ProductionPlan out.
//
// Pure computation over the recipe and prototype registries — no geometry and
// no I/O. `ProductionPlan` is the entire interface to the layout phase, which
// is what lets the ratios be tested with no layout code in existence.
use std::collections::{HashMap, HashSet};

use factorio_grid::prototype::{self, EntityPrototype};

use crate::availability::Availability;
use crate::recipe::Recipe;

pub mod error;
pub mod select;
pub mod solve;

pub use error::ChainError;
pub use solve::solve;

// ── Goal ──────────────────────────────────────────────────────────────

/// A target throughput, in whichever unit the user thinks in.
#[derive(Debug, Clone, PartialEq)]
pub enum Rate {
    ItemsPerSec(f64),
    ItemsPerMin(f64),
    /// `count` full belts of `tier`, e.g. 2 × `express-transport-belt`.
    Belts { count: u32, tier: String },
}

impl Rate {
    /// Items per second. Fallible because `Belts` resolves its tier through
    /// the prototype registry: an unknown name is an error rather than a
    /// silent default, the same never-guess rule as recipe selection.
    pub fn per_sec(&self) -> Result<f64, ChainError> {
        match self {
            Rate::ItemsPerSec(n) => Ok(*n),
            Rate::ItemsPerMin(n) => Ok(n / 60.0),
            Rate::Belts { count, tier } => {
                let throughput = prototype::lookup(tier)
                    .and_then(|p| p.belt_throughput)
                    .ok_or_else(|| ChainError::UnknownBeltTier(tier.clone()))?;
                Ok(f64::from(*count) * throughput)
            }
        }
    }
}

/// What to use for a crafting category with no explicit preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineFallback {
    /// Highest `crafting_speed` prototype covering the category.
    FastestAvailable,
    /// A specific machine, used wherever its categories allow.
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachinePolicy {
    /// Crafting category → machine name, e.g. `electronics` →
    /// `assembling-machine-2`.
    pub preferred: HashMap<String, String>,
    pub fallback: MachineFallback,
}

impl MachinePolicy {
    /// Fastest machine covering each category — the default policy.
    pub fn fastest() -> Self {
        Self { preferred: HashMap::new(), fallback: MachineFallback::FastestAvailable }
    }

    /// Use `machine` for every category it can craft. Categories it cannot
    /// craft still error rather than quietly falling back to something else:
    /// "build it all in assembler 2" is a claim about the whole plan, so a
    /// category that breaks it is worth surfacing.
    pub fn all(machine: &str) -> Self {
        Self {
            preferred: HashMap::new(),
            fallback: MachineFallback::Named(machine.to_string()),
        }
    }

    /// Pin one category to one machine, leaving the rest to the fallback.
    pub fn with_preference(mut self, category: &str, machine: &str) -> Self {
        self.preferred.insert(category.to_string(), machine.to_string());
        self
    }
}

impl Default for MachinePolicy {
    fn default() -> Self {
        Self::fastest()
    }
}

/// What to build, how fast, and where the chain stops.
///
/// `available` is the load-bearing field: it is the bus, and resolution walks
/// back from `product` until it reaches it. The same code path serves "one
/// assembler" and "the whole chain" — the only difference is how much the
/// user says they already have.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainGoal {
    pub product: String,
    pub rate: Rate,
    /// Items already on the bus. Resolution stops here.
    pub available: HashSet<String>,
    pub machines: MachinePolicy,
    /// Item → recipe name, resolving items with more than one producer.
    pub recipe_overrides: HashMap<String, String>,
    /// What the player can build. Defaults to `Everything`, so every existing
    /// caller is behaviour-identical apart from the never-unlockable recipes.
    pub availability: Availability,
}

impl ChainGoal {
    /// A goal with the default machine policy, no overrides, and every
    /// buildable recipe available (see `Availability::Everything`).
    pub fn new(product: &str, rate: Rate, available: &[&str]) -> Self {
        Self {
            product: product.to_string(),
            rate,
            available: available.iter().map(|s| s.to_string()).collect(),
            machines: MachinePolicy::fastest(),
            recipe_overrides: HashMap::new(),
            availability: Availability::Everything,
        }
    }

    pub fn with_machines(mut self, machines: MachinePolicy) -> Self {
        self.machines = machines;
        self
    }

    pub fn with_override(mut self, item: &str, recipe: &str) -> Self {
        self.recipe_overrides.insert(item.to_string(), recipe.to_string());
        self
    }

    /// Overrides the default `Everything` — what a specific player can
    /// actually build, from a save import, a technology projection, or the
    /// UI's own edited set.
    pub fn with_availability(mut self, availability: Availability) -> Self {
        self.availability = availability;
        self
    }
}

// ── Plan ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ItemRate {
    pub item: String,
    pub per_sec: f64,
}

impl ItemRate {
    pub fn new(item: &str, per_sec: f64) -> Self {
        Self { item: item.to_string(), per_sec }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductionStep {
    pub recipe: &'static Recipe,
    pub machine: &'static EntityPrototype,
    /// The true, fractional machine count — the real ratio.
    pub exact_count: f64,
    /// `exact_count.ceil()` — what you actually build.
    pub machines_needed: u32,
    pub crafts_per_sec: f64,
    pub inputs: Vec<ItemRate>,
    pub outputs: Vec<ItemRate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductionPlan {
    /// Topologically ordered, producers before consumers, so the layout
    /// phase can place them in a single forward pass.
    pub steps: Vec<ProductionStep>,
    /// What the bus must supply.
    pub inputs: Vec<ItemRate>,
    /// Surplus the player has to deal with.
    pub byproducts: Vec<ItemRate>,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_conversions() {
        assert_eq!(Rate::ItemsPerSec(45.0).per_sec().unwrap(), 45.0);
        assert_eq!(Rate::ItemsPerMin(60.0).per_sec().unwrap(), 1.0);
        // "2 blue belts" == 90/s, from express-transport-belt's throughput of 45.
        assert_eq!(
            Rate::Belts { count: 2, tier: "express-transport-belt".into() }.per_sec().unwrap(),
            90.0
        );
        assert_eq!(
            Rate::Belts { count: 1, tier: "transport-belt".into() }.per_sec().unwrap(),
            15.0
        );
    }

    #[test]
    fn unknown_belt_tier_errors_rather_than_defaulting() {
        assert!(matches!(
            Rate::Belts { count: 1, tier: "not-a-belt".into() }.per_sec(),
            Err(ChainError::UnknownBeltTier(_))
        ));
    }

    #[test]
    fn a_real_entity_that_is_not_a_belt_is_still_rejected() {
        // assembling-machine-2 exists but has no belt_throughput — being a
        // known prototype is not enough.
        assert!(matches!(
            Rate::Belts { count: 1, tier: "assembling-machine-2".into() }.per_sec(),
            Err(ChainError::UnknownBeltTier(_))
        ));
    }

    #[test]
    fn errors_display_actionably() {
        let e = ChainError::FluidIngredient {
            recipe: "plastic-bar".into(),
            fluid: "petroleum-gas".into(),
        };
        let s = e.to_string();
        assert!(s.contains("plastic-bar") && s.contains("petroleum-gas"), "{s}");
    }

    #[test]
    fn machine_policy_constructors() {
        assert_eq!(MachinePolicy::fastest().fallback, MachineFallback::FastestAvailable);
        assert_eq!(
            MachinePolicy::all("assembling-machine-2").fallback,
            MachineFallback::Named("assembling-machine-2".into())
        );
        let p = MachinePolicy::fastest().with_preference("electronics", "assembling-machine-2");
        assert_eq!(p.preferred.get("electronics").unwrap(), "assembling-machine-2");
    }

    #[test]
    fn goal_builders_populate_the_boundary() {
        let g = ChainGoal::new("electronic-circuit", Rate::ItemsPerSec(45.0), &["iron-plate"])
            .with_override("copper-cable", "copper-cable");
        assert!(g.available.contains("iron-plate"));
        assert_eq!(g.recipe_overrides.get("copper-cable").unwrap(), "copper-cable");
    }
}
