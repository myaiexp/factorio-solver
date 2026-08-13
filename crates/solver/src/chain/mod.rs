// Production-chain calculator: a ChainGoal in, a ProductionPlan out.
//
// Pure computation over the recipe and prototype registries — no geometry and
// no I/O. `ProductionPlan` is the entire interface to the layout phase, which
// is what lets the ratios be tested with no layout code in existence.
use std::collections::{HashMap, HashSet};

use factorio_grid::prototype::{self, EntityPrototype};

use crate::recipe::{self, Recipe};

pub mod error;
pub mod select;
pub mod solve;

pub use error::ChainError;
pub use solve::solve;

// ── Availability ──────────────────────────────────────────────────────

/// What the player can actually build, read from a save's unlocked-recipe
/// set. Kept separate from `available` (the bus) — one is "what item I
/// already have", the other is "what I'm allowed to craft at all".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Availability {
    /// Everything. The default — preserves existing behaviour exactly, so a
    /// caller that never sets this sees no change from before the feature.
    #[default]
    Unrestricted,
    /// Only these recipe names.
    Unlocked(HashSet<String>),
}

impl Availability {
    pub fn allows_recipe(&self, recipe: &str) -> bool {
        match self {
            Availability::Unrestricted => true,
            Availability::Unlocked(names) => names.contains(recipe),
        }
    }

    /// A machine is craftable when some allowed recipe produces an item
    /// named `machine`. Keyed on the produced *item*, not the recipe name,
    /// because that is what `select_machine` has in hand — an
    /// `EntityPrototype` name, not a recipe.
    ///
    /// This assumes entity-prototype name and item name coincide, which
    /// holds for every machine in this data set but is an assumption, not a
    /// guarantee: a machine that violates it would read as locked rather
    /// than as wrongly available, which is the safer failure direction.
    pub fn allows_machine(&self, machine: &str) -> bool {
        match self {
            Availability::Unrestricted => true,
            Availability::Unlocked(_) => recipe::registry()
                .values()
                .filter(|r| self.allows_recipe(&r.name))
                .any(|r| r.results.iter().any(|res| res.name == machine)),
        }
    }
}

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
    /// What the player can actually build. Defaults to `Unrestricted`, so
    /// every existing caller of `new` is unaffected until it opts in.
    pub availability: Availability,
}

impl ChainGoal {
    /// A goal with the default machine policy, no overrides, and no
    /// availability restriction.
    pub fn new(product: &str, rate: Rate, available: &[&str]) -> Self {
        Self {
            product: product.to_string(),
            rate,
            available: available.iter().map(|s| s.to_string()).collect(),
            machines: MachinePolicy::fastest(),
            recipe_overrides: HashMap::new(),
            availability: Availability::Unrestricted,
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
mod tests;
