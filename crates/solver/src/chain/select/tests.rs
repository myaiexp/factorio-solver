// Tests for recipe/machine selection, including availability filtering.
// Split out of `select.rs` to keep it under the file-length limit.
use super::*;
use std::collections::HashSet;

/// Builds `Availability::Unlocked` from a slice of recipe names — used
/// throughout these tests the way a real save's unlocked set would be.
fn unlocked(names: &[&str]) -> Availability {
    Availability::Unlocked(names.iter().map(|s| s.to_string()).collect())
}

#[test]
fn single_result_recipe_resolves_without_override() {
    let r = select_recipe("electronic-circuit", &HashMap::new(), &Availability::Unrestricted)
        .expect("resolves");
    assert_eq!(r.name, "electronic-circuit");
}

#[test]
fn copper_cable_is_ambiguous_and_errors_with_candidates() {
    let candidates = candidates_for("copper-cable");
    let names: Vec<&str> = candidates.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"copper-cable"), "{names:?}");
    assert!(names.contains(&"casting-copper-cable"), "{names:?}");
    assert_eq!(candidates.len(), 2, "a 3rd means the scrap-recycling filter regressed: {names:?}");

    match select_recipe("copper-cable", &HashMap::new(), &Availability::Unrestricted) {
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
    let r = select_recipe("copper-cable", &overrides, &Availability::Unrestricted)
        .expect("resolves via override");
    assert_eq!(r.name, "copper-cable");
}

#[test]
fn override_naming_a_nonexistent_recipe_errors() {
    let mut overrides = HashMap::new();
    overrides.insert("copper-cable".to_string(), "not-a-real-recipe".to_string());
    assert!(matches!(
        select_recipe("copper-cable", &overrides, &Availability::Unrestricted),
        Err(ChainError::UnknownItem(name)) if name == "not-a-real-recipe"
    ));
}

#[test]
fn locked_recipe_errors_with_the_locked_candidates_named() {
    // Both copper-cable producers are locked: RecipeLocked, not
    // AmbiguousRecipe — there is nothing to choose between, only
    // something to unlock.
    let a = unlocked(&["iron-plate"]);
    match select_recipe("copper-cable", &HashMap::new(), &a) {
        Err(ChainError::RecipeLocked { item, recipes }) => {
            assert_eq!(item, "copper-cable");
            assert!(recipes.contains(&"copper-cable".to_string()));
            assert!(recipes.contains(&"casting-copper-cable".to_string()));
        }
        other => panic!("expected RecipeLocked, got {other:?}"),
    }
}

#[test]
fn availability_narrows_ambiguity_down_to_a_single_unlocked_recipe() {
    let a = unlocked(&["copper-cable"]);
    let r = select_recipe("copper-cable", &HashMap::new(), &a)
        .expect("only one unlocked producer remains");
    assert_eq!(r.name, "copper-cable");
}

#[test]
fn availability_can_leave_a_candidate_list_still_ambiguous() {
    // Both copper-cable producers unlocked: still two candidates, so
    // still AmbiguousRecipe — availability narrows, it doesn't always
    // resolve.
    let a = unlocked(&["copper-cable", "casting-copper-cable"]);
    match select_recipe("copper-cable", &HashMap::new(), &a) {
        Err(ChainError::AmbiguousRecipe { candidates, .. }) => {
            assert_eq!(candidates.len(), 2, "{candidates:?}");
        }
        other => panic!("expected AmbiguousRecipe, got {other:?}"),
    }
}

#[test]
fn override_wins_even_when_the_recipe_is_locked() {
    let a = unlocked(&["iron-plate"]); // copper-cable itself is locked
    let mut overrides = HashMap::new();
    overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
    let r = select_recipe("copper-cable", &overrides, &a)
        .expect("an explicit override bypasses availability");
    assert_eq!(r.name, "copper-cable");
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
    let machine = select_machine(recipe, &MachinePolicy::fastest(), &Availability::Unrestricted)
        .expect("selects a machine");
    assert_eq!(machine.name, "electromagnetic-plant");
}

#[test]
fn named_fallback_is_used_when_it_covers_the_category() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let machine = select_machine(
        recipe,
        &MachinePolicy::all("assembling-machine-2"),
        &Availability::Unrestricted,
    )
    .expect("selects a machine");
    assert_eq!(machine.name, "assembling-machine-2");
}

#[test]
fn named_machine_that_cannot_craft_the_category_errors() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    match select_machine(recipe, &MachinePolicy::all("stone-furnace"), &Availability::Unrestricted) {
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
    let machine =
        select_machine(recipe, &policy, &Availability::Unrestricted).expect("selects a machine");
    assert_eq!(machine.name, "assembling-machine-3");
}

#[test]
fn unknown_machine_name_errors() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    assert!(matches!(
        select_machine(recipe, &MachinePolicy::all("not-a-machine"), &Availability::Unrestricted),
        Err(ChainError::UnknownMachine { machine }) if machine == "not-a-machine"
    ));
}

#[test]
fn candidate_order_is_deterministic() {
    let a: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
    let b: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
    assert_eq!(a, b);
}

#[test]
fn fastest_policy_falls_back_past_a_locked_machine() {
    // electromagnetic-plant (2.0) and assembling-machine-3 (1.25) both
    // cover "electronics" and both beat assembling-machine-2 (0.75) on
    // speed, but neither is unlocked here, so the fastest *reachable*
    // choice is assembling-machine-2.
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let a = unlocked(&["assembling-machine-1", "assembling-machine-2"]);
    let m = select_machine(recipe, &MachinePolicy::fastest(), &a).unwrap();
    assert_eq!(m.name, "assembling-machine-2");
}

#[test]
fn named_locked_machine_errors_rather_than_downgrading() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let a = unlocked(&["assembling-machine-1"]);
    match select_machine(recipe, &MachinePolicy::all("assembling-machine-3"), &a) {
        Err(ChainError::MachineLocked { machine, recipe: recipe_name }) => {
            assert_eq!(machine, "assembling-machine-3");
            assert_eq!(recipe_name, "electronic-circuit");
        }
        other => panic!("expected MachineLocked, got {other:?}"),
    }
}

#[test]
fn preferred_category_locked_machine_errors_the_same_as_a_named_fallback() {
    // A per-category preference is just as much a deliberate statement
    // as `MachinePolicy::all` — locking its machine must error the same
    // way, not silently fall through to the (unlocked) global fallback.
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let policy = MachinePolicy::all("assembling-machine-2")
        .with_preference("electronics", "assembling-machine-3");
    let a = unlocked(&["assembling-machine-2"]); // assembling-machine-3 locked
    match select_machine(recipe, &policy, &a) {
        Err(ChainError::MachineLocked { machine, .. }) => assert_eq!(machine, "assembling-machine-3"),
        other => panic!("expected MachineLocked, got {other:?}"),
    }
}

#[test]
fn unavailable_category_with_no_unlocked_machine_reports_no_machine_for_category() {
    // Every prototype that covers "electronics" is locked: this is a
    // capability gap, not a single named machine to blame, so the
    // fastest-search path reports NoMachineForCategory, not
    // MachineLocked.
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let a = Availability::Unlocked(HashSet::new());
    match select_machine(recipe, &MachinePolicy::fastest(), &a) {
        Err(ChainError::NoMachineForCategory { category, .. }) => assert_eq!(category, "electronics"),
        other => panic!("expected NoMachineForCategory, got {other:?}"),
    }
}
