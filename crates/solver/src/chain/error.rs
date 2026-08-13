// Errors from chain resolution — every variant names the fix, not just the fault.
use thiserror::Error;

/// Every variant names the offending item/recipe **and** the remedy.
///
/// These messages are rendered verbatim in the UI: a chain that silently
/// guesses produces a blueprint that looks right and wastes real in-game
/// time to discover, so ambiguity is always an error and never a default.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ChainError {
    #[error("'{0}' is not a known item — check the spelling, or it may be from a mod")]
    UnknownItem(String),

    #[error(
        "'{0}' is not a belt — give a belt prototype name such as \
         'transport-belt', 'fast-transport-belt', 'express-transport-belt' or \
         'turbo-transport-belt', or state the rate in items per second"
    )]
    UnknownBeltTier(String),

    #[error(
        "recipe '{recipe}' needs the fluid '{fluid}', which cannot be carried on a belt — \
         declare '{recipe}'s product available on the bus so the chain stops before it, \
         or pick a goal that does not need fluids"
    )]
    FluidIngredient { recipe: String, fluid: String },

    #[error(
        "'{item}' can be made by more than one recipe ({}) — \
         choose one with a recipe override for '{item}'",
        .candidates.join(", ")
    )]
    AmbiguousRecipe { item: String, candidates: Vec<String> },

    #[error(
        "no available machine can craft category '{category}' (needed by recipe '{recipe}') — \
         pick a machine for that category in the machine policy"
    )]
    NoMachineForCategory { category: String, recipe: String },

    #[error(
        "'{item}' has producers ({}), but none are unlocked in the loaded save — \
         research one of them, add '{item}' to what's available so the chain stops there, \
         or clear the save's recipe selection to plan without that restriction",
        .recipes.join(", ")
    )]
    RecipeLocked { item: String, recipes: Vec<String> },

    #[error(
        "'{machine}' can craft recipe '{recipe}', but is not unlocked in the loaded save — \
         the machine category is fine, it just hasn't been researched yet: research '{machine}', \
         or point the machine policy at a machine that is already unlocked"
    )]
    MachineLocked { machine: String, recipe: String },

    #[error(
        "'{item}' cannot be reached from what is available — it has no recipe and is not a raw \
         resource, so declare it available on the bus"
    )]
    UnreachableBoundary { item: String },

    #[error(
        "'{machine}' is not a known entity — pick a machine that exists, \
         or it may be from a mod"
    )]
    UnknownMachine { machine: String },

    #[error(
        "the rates for '{product}' did not settle after {iterations} passes — this chain has a \
         feedback loop that consumes more than it produces, so no steady state exists"
    )]
    DidNotConverge { product: String, iterations: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_error_names_recipe_fluid_and_remedy() {
        let msg = ChainError::FluidIngredient {
            recipe: "plastic-bar".into(),
            fluid: "petroleum-gas".into(),
        }
        .to_string();
        assert!(msg.contains("plastic-bar"), "{msg}");
        assert!(msg.contains("petroleum-gas"), "{msg}");
        assert!(msg.contains("available"), "must state the fix: {msg}");
    }

    #[test]
    fn ambiguous_error_lists_every_candidate() {
        let msg = ChainError::AmbiguousRecipe {
            item: "copper-cable".into(),
            candidates: vec!["copper-cable".into(), "casting-copper-cable".into()],
        }
        .to_string();
        assert!(msg.contains("copper-cable") && msg.contains("casting-copper-cable"), "{msg}");
        assert!(msg.contains("override"), "must state the fix: {msg}");
    }

    #[test]
    fn no_machine_error_names_category_and_recipe() {
        let msg = ChainError::NoMachineForCategory {
            category: "chemistry".into(),
            recipe: "sulfur".into(),
        }
        .to_string();
        assert!(msg.contains("chemistry") && msg.contains("sulfur"), "{msg}");
    }

    #[test]
    fn belt_tier_error_suggests_real_belt_names() {
        let msg = ChainError::UnknownBeltTier("not-a-belt".into()).to_string();
        assert!(msg.contains("not-a-belt") && msg.contains("express-transport-belt"), "{msg}");
    }

    #[test]
    fn recipe_locked_names_the_item_candidates_and_remedy() {
        let msg = ChainError::RecipeLocked {
            item: "copper-cable".into(),
            recipes: vec!["copper-cable".into(), "casting-copper-cable".into()],
        }
        .to_string();
        assert!(msg.contains("copper-cable") && msg.contains("casting-copper-cable"), "{msg}");
        assert!(msg.contains("research") || msg.contains("available"), "must state the fix: {msg}");
    }

    #[test]
    fn machine_locked_names_the_machine_and_does_not_suggest_a_different_category() {
        let msg = ChainError::MachineLocked {
            machine: "assembling-machine-3".into(),
            recipe: "electronic-circuit".into(),
        }
        .to_string();
        assert!(msg.contains("assembling-machine-3") && msg.contains("electronic-circuit"), "{msg}");
        assert!(msg.contains("research"), "must state the fix: {msg}");
        // Distinct remedy from NoMachineForCategory: the category is fine,
        // so the message must not tell the player to pick a new category.
        assert!(
            !msg.contains("pick a machine for that category"),
            "MachineLocked must not reuse NoMachineForCategory's remedy: {msg}"
        );
    }

    #[test]
    fn every_variant_is_a_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&ChainError::UnknownItem("x".into()));
    }
}
