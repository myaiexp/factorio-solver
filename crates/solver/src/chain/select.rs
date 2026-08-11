// Selection layer: which recipe makes an item, and which machine crafts a recipe.
use std::collections::HashMap;

use factorio_grid::prototype::{self, EntityPrototype};

use crate::chain::{ChainError, MachineFallback, MachinePolicy};
use crate::recipe::{self, Recipe};

/// Every recipe that can be chosen to produce `item`.
///
/// Cheap and pure by design: it runs once per item during chain resolution,
/// so it just walks the registry rather than maintaining a cache that could
/// go stale relative to it.
pub fn candidates_for(item: &str) -> Vec<&'static Recipe> {
    let mut out: Vec<&'static Recipe> = recipe::registry()
        .values()
        .filter(|r| r.results.iter().any(|res| res.name == item))
        .filter(|r| !r.hidden)
        // `starts_with`, not `==`: "recycling" (310 recipes) is not the only
        // recycling category. `scrap-recycling` is
        // "recycling-or-hand-crafting" and outputs 10+ common items
        // (copper-cable, iron-plate, ...); an exact-equality filter lets it
        // through and makes almost every common item look ambiguous.
        .filter(|r| !r.category.starts_with("recycling"))
        // `main_product` only ever demotes: `Some(p)` with `p != item` proves
        // `item` is a secondary result of this recipe. It can never promote —
        // requiring `main_product == item` would drop recipes like
        // uranium-processing, which declares no main_product at all, as a
        // candidate for *either* of its own outputs.
        .filter(|r| !matches!(&r.main_product, Some(p) if p != item))
        .collect();
    // `registry()` is a HashMap; iteration order is otherwise random, which
    // would make `AmbiguousRecipe`'s candidate list differ between runs.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Pick the one recipe that produces `item`: an override wins outright,
/// otherwise there must be exactly one candidate. Never guesses between
/// several — that is always `AmbiguousRecipe`.
pub fn select_recipe(
    item: &str,
    overrides: &HashMap<String, String>,
) -> Result<&'static Recipe, ChainError> {
    if let Some(recipe_name) = overrides.get(item) {
        return recipe::get(recipe_name).ok_or_else(|| ChainError::UnknownItem(recipe_name.clone()));
    }

    let mut candidates = candidates_for(item);
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(ChainError::UnreachableBoundary { item: item.to_string() }),
        _ => Err(ChainError::AmbiguousRecipe {
            item: item.to_string(),
            candidates: candidates.into_iter().map(|r| r.name.clone()).collect(),
        }),
    }
}

/// Pick the machine that crafts `recipe`'s category, honouring `policy`.
pub fn select_machine(
    recipe: &Recipe,
    policy: &MachinePolicy,
) -> Result<&'static EntityPrototype, ChainError> {
    let category = &recipe.category;

    let named = policy.preferred.get(category).or(match &policy.fallback {
        MachineFallback::Named(m) => Some(m),
        MachineFallback::FastestAvailable => None,
    });

    if let Some(machine) = named {
        let proto = prototype::lookup(machine)
            .ok_or_else(|| ChainError::UnknownMachine { machine: machine.clone() })?;
        return if proto.crafting_categories.iter().any(|c| c == category) {
            Ok(proto)
        } else {
            Err(ChainError::NoMachineForCategory {
                category: category.clone(),
                recipe: recipe.name.clone(),
            })
        };
    }

    // No preference and no named fallback: search for the fastest prototype
    // covering the category. `all_names()` iterates a HashMap, so ties are
    // broken by name to keep the result deterministic.
    let mut best: Option<&'static EntityPrototype> = None;
    for name in prototype::all_names() {
        let Some(proto) = prototype::lookup(name) else { continue };
        if !proto.crafting_categories.iter().any(|c| c == category) {
            continue;
        }
        let Some(speed) = proto.crafting_speed else { continue };
        best = match best {
            None => Some(proto),
            Some(current) => {
                let current_speed = current.crafting_speed.unwrap_or(0.0);
                if speed > current_speed || (speed == current_speed && proto.name < current.name) {
                    Some(proto)
                } else {
                    Some(current)
                }
            }
        };
    }

    best.ok_or_else(|| ChainError::NoMachineForCategory {
        category: category.clone(),
        recipe: recipe.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_result_recipe_resolves_without_override() {
        let r = select_recipe("electronic-circuit", &HashMap::new()).expect("resolves");
        assert_eq!(r.name, "electronic-circuit");
    }

    #[test]
    fn copper_cable_is_ambiguous_and_errors_with_candidates() {
        let candidates = candidates_for("copper-cable");
        let names: Vec<&str> = candidates.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"copper-cable"), "{names:?}");
        assert!(names.contains(&"casting-copper-cable"), "{names:?}");
        assert_eq!(
            candidates.len(),
            2,
            "a 3rd means the scrap-recycling filter regressed: {names:?}"
        );

        match select_recipe("copper-cable", &HashMap::new()) {
            Err(ChainError::AmbiguousRecipe { item, candidates }) => {
                assert_eq!(item, "copper-cable");
                assert!(candidates.contains(&"copper-cable".to_string()));
                assert!(candidates.contains(&"casting-copper-cable".to_string()));
            }
            other => panic!("expected AmbiguousRecipe, got {other:?}"),
        }
    }

    #[test]
    fn override_resolves_ambiguity() {
        let mut overrides = HashMap::new();
        overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
        let r = select_recipe("copper-cable", &overrides).expect("resolves via override");
        assert_eq!(r.name, "copper-cable");
    }

    #[test]
    fn override_naming_a_nonexistent_recipe_errors() {
        let mut overrides = HashMap::new();
        overrides.insert("copper-cable".to_string(), "not-a-real-recipe".to_string());
        assert!(matches!(
            select_recipe("copper-cable", &overrides),
            Err(ChainError::UnknownItem(name)) if name == "not-a-real-recipe"
        ));
    }

    #[test]
    fn recycling_recipes_are_never_candidates() {
        let candidates = candidates_for("iron-plate");
        assert!(candidates.iter().all(|r| !r.category.starts_with("recycling")));
        assert!(
            !candidates.iter().any(|r| r.name == "scrap-recycling"),
            "scrap-recycling is category 'recycling-or-hand-crafting', which an \
             exact-equality filter would miss"
        );
    }

    #[test]
    fn research_locked_recipes_are_still_candidates() {
        let candidates = candidates_for("uranium-235");
        assert!(
            candidates.iter().any(|r| r.name == "uranium-processing"),
            "enabled:false must not exclude a recipe from being a candidate"
        );
    }

    #[test]
    fn multi_output_recipe_produces_all_its_outputs() {
        // uranium-processing declares no main_product, so it is a candidate
        // for both of its results.
        assert!(candidates_for("uranium-235").iter().any(|r| r.name == "uranium-processing"));
        assert!(candidates_for("uranium-238").iter().any(|r| r.name == "uranium-processing"));
    }

    #[test]
    fn declared_main_product_demotes_the_other_results() {
        // copper-bacteria declares main_product "copper-bacteria" but also
        // produces "spoilage" as a side effect. The recipe is a candidate
        // for its main product but not for the secondary one.
        let cb = recipe::get("copper-bacteria").expect("copper-bacteria exists");
        assert!(cb.results.iter().any(|res| res.name == "spoilage"), "test assumption");

        assert!(candidates_for("copper-bacteria").iter().any(|r| r.name == "copper-bacteria"));
        assert!(
            !candidates_for("spoilage").iter().any(|r| r.name == "copper-bacteria"),
            "main_product demotes spoilage as copper-bacteria's product"
        );
    }

    #[test]
    fn machine_selection_matches_crafting_category() {
        let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
        assert_eq!(recipe.category, "electronics");
        let machine = select_machine(recipe, &MachinePolicy::fastest()).expect("selects a machine");
        assert_eq!(machine.name, "electromagnetic-plant");
    }

    #[test]
    fn named_fallback_is_used_when_it_covers_the_category() {
        let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
        let machine = select_machine(recipe, &MachinePolicy::all("assembling-machine-2"))
            .expect("selects a machine");
        assert_eq!(machine.name, "assembling-machine-2");
    }

    #[test]
    fn named_machine_that_cannot_craft_the_category_errors() {
        let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
        match select_machine(recipe, &MachinePolicy::all("stone-furnace")) {
            Err(ChainError::NoMachineForCategory { category, recipe: recipe_name }) => {
                assert_eq!(category, "electronics");
                assert_eq!(recipe_name, "electronic-circuit");
            }
            other => panic!("expected NoMachineForCategory, got {other:?}"),
        }
    }

    #[test]
    fn preferred_category_beats_the_fallback() {
        let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
        let policy = MachinePolicy::all("assembling-machine-2")
            .with_preference("electronics", "assembling-machine-3");
        let machine = select_machine(recipe, &policy).expect("selects a machine");
        assert_eq!(machine.name, "assembling-machine-3");
    }

    #[test]
    fn unknown_machine_name_errors() {
        let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
        assert!(matches!(
            select_machine(recipe, &MachinePolicy::all("not-a-machine")),
            Err(ChainError::UnknownMachine { machine }) if machine == "not-a-machine"
        ));
    }

    #[test]
    fn candidate_order_is_deterministic() {
        let a: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
        let b: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
        assert_eq!(a, b);
    }
}
