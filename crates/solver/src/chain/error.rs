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

    #[error("'{item}' cannot be built yet: {}", unlock_hint(.unlocked_by))]
    NotUnlocked { item: String, unlocked_by: Vec<String> },

    #[error(
        "no available machine can craft category '{category}' (needed by recipe '{recipe}') — \
         pick a machine for that category in the machine policy"
    )]
    NoMachineForCategory { category: String, recipe: String },

    #[error("machine '{machine}' cannot be built yet: {}", unlock_hint(.unlocked_by))]
    MachineNotUnlocked { machine: String, unlocked_by: Vec<String> },

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

/// Renders the "what would unlock this" half of a not-unlocked message.
/// Empty is real, not a bug: eight recipes have no unlocking technology at
/// all, so the remedy there is the panel, never research.
fn unlock_hint(unlocked_by: &[String]) -> String {
    match unlocked_by {
        [] => "no technology unlocks it — tick it in the available-recipes list if you have it \
               anyway"
            .to_string(),
        names => format!(
            "it needs {} — research it, or tick the recipe in the available-recipes list",
            names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(" or ")
        ),
    }
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
    fn not_unlocked_error_names_item_technology_and_both_remedies() {
        let msg = ChainError::NotUnlocked {
            item: "copper-cable".into(),
            unlocked_by: vec!["electronics".into(), "foundry".into()],
        }
        .to_string();
        assert!(msg.contains("copper-cable"), "{msg}");
        assert!(msg.contains("electronics") && msg.contains("foundry"), "{msg}");
        assert!(msg.contains("research"), "must state the research remedy: {msg}");
        assert!(msg.contains("available-recipes list"), "must state the tick-it remedy: {msg}");
    }

    #[test]
    fn not_unlocked_error_with_no_unlockers_still_reads_as_a_sentence() {
        // The eight recipes no technology unlocks are a real case, not a
        // bug — the message must not trail off just because the list is empty.
        let msg = ChainError::NotUnlocked { item: "loader".into(), unlocked_by: vec![] }.to_string();
        assert!(msg.contains("loader"), "{msg}");
        assert!(!msg.contains("research it"), "no technology exists to research: {msg}");
        assert!(msg.contains("no technology unlocks it"), "{msg}");
        assert!(msg.contains("available-recipes list"), "must still state a remedy: {msg}");
    }

    #[test]
    fn machine_not_unlocked_error_names_machine_and_technology() {
        let msg = ChainError::MachineNotUnlocked {
            machine: "electromagnetic-plant".into(),
            unlocked_by: vec!["electromagnetic-plant".into()],
        }
        .to_string();
        assert!(msg.contains("electromagnetic-plant"), "{msg}");
        assert!(msg.contains("research"), "must state the remedy: {msg}");
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
    fn every_variant_is_a_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&ChainError::UnknownItem("x".into()));
    }
}
