// Tests for `select_machine`: category coverage, the named/preferred/
// fallback policy, and availability's effect on all three. Split out of
// `select_tests.rs` (recipe selection) to keep each file under the length
// cap.
use super::*;

/// Builds `Availability::Unlocked` from a slice of recipe names.
fn unlocked(names: &[&str]) -> Availability {
    Availability::Unlocked(names.iter().map(|n| n.to_string()).collect())
}

#[test]
fn machine_selection_matches_crafting_category() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    assert_eq!(recipe.category, "electronics");
    let machine = select_machine(recipe, &MachinePolicy::fastest(), &Availability::Everything)
        .expect("selects a machine");
    assert_eq!(machine.name, "electromagnetic-plant");
}

#[test]
fn named_fallback_is_used_when_it_covers_the_category() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    let machine = select_machine(
        recipe,
        &MachinePolicy::all("assembling-machine-2"),
        &Availability::Everything,
    )
    .expect("selects a machine");
    assert_eq!(machine.name, "assembling-machine-2");
}

#[test]
fn named_machine_that_cannot_craft_the_category_errors() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    match select_machine(recipe, &MachinePolicy::all("stone-furnace"), &Availability::Everything) {
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
        select_machine(recipe, &policy, &Availability::Everything).expect("selects a machine");
    assert_eq!(machine.name, "assembling-machine-3");
}

#[test]
fn unknown_machine_name_errors() {
    let recipe = recipe::get("electronic-circuit").expect("electronic-circuit exists");
    assert!(matches!(
        select_machine(recipe, &MachinePolicy::all("not-a-machine"), &Availability::Everything),
        Err(ChainError::UnknownMachine { machine }) if machine == "not-a-machine"
    ));
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
        Err(ChainError::MachineLocked { machine, recipe: recipe_name, .. }) => {
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

/// `metallurgy` is covered by exactly one prototype, `foundry`, which is
/// locked here. This used to be `NoMachineForCategory`, but that error now
/// means only "nothing covers this category at all, at any availability" —
/// `select_machine`'s fastest-available search tracks `best_overall`
/// alongside `best_available` precisely so a category with a real (if
/// locked) machine can name it instead, pointing the player at research
/// rather than at the machine policy.
#[test]
fn every_machine_for_a_category_locked_names_the_fastest_one() {
    let recipe = recipe::get("casting-iron").expect("casting-iron exists");
    assert_eq!(recipe.category, "metallurgy");
    match select_machine(recipe, &MachinePolicy::fastest(), &unlocked(&[])) {
        Err(ChainError::MachineLocked { machine, unlocked_by, .. }) => {
            assert_eq!(machine, "foundry");
            assert_eq!(unlocked_by, vec!["foundry".to_string()]);
        }
        other => panic!("expected MachineLocked naming foundry, got {other:?}"),
    }
}
