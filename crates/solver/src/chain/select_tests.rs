// Tests for recipe selection: candidate lists, `select_recipe`, and
// availability's effect on both. Machine selection is split out to
// `select_machine_tests.rs` to keep this file under the length cap.
use super::*;

/// Builds `Availability::Unlocked` from a slice of recipe names.
fn unlocked(names: &[&str]) -> Availability {
    Availability::Unlocked(names.iter().map(|n| n.to_string()).collect())
}

#[test]
fn single_result_recipe_resolves_without_override() {
    let r = select_recipe("electronic-circuit", &HashMap::new(), &Availability::Everything)
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

    match select_recipe("copper-cable", &HashMap::new(), &Availability::Everything) {
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
    let r = select_recipe("copper-cable", &overrides, &Availability::Everything)
        .expect("resolves via override");
    assert_eq!(r.name, "copper-cable");
}

#[test]
fn override_naming_a_nonexistent_recipe_errors() {
    let mut overrides = HashMap::new();
    overrides.insert("copper-cable".to_string(), "not-a-real-recipe".to_string());
    assert!(matches!(
        select_recipe("copper-cable", &overrides, &Availability::Everything),
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
fn candidate_order_is_deterministic() {
    let a: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
    let b: Vec<&str> = candidates_for("copper-cable").iter().map(|r| r.name.as_str()).collect();
    assert_eq!(a, b);
}

// ── Availability gating ─────────────────────────────────────────────────

#[test]
fn available_candidates_for_is_a_subset_of_candidates_for() {
    let restrictive = unlocked(&["electronic-circuit"]);
    for item in ["copper-cable", "iron-plate", "iron-gear-wheel", "pipe", "uranium-235"] {
        let all: Vec<&str> = candidates_for(item).iter().map(|r| r.name.as_str()).collect();
        for availability in [&restrictive, &Availability::Everything] {
            let gated: Vec<&str> =
                available_candidates_for(item, availability).iter().map(|r| r.name.as_str()).collect();
            assert!(
                gated.iter().all(|n| all.contains(n)),
                "{item} under {availability:?}: {gated:?} not a subset of {all:?}"
            );
        }
    }
}

/// The never-unlockable exclusion changes nothing here, and that is worth an
/// assertion rather than a discovery: every one of those eight recipes is
/// **also** `hidden: true`, so `candidates_for`'s long-standing hidden filter
/// already removed them and availability finds nothing left to remove. The
/// exclusion earns its keep in `availability::all_available_recipe_names`,
/// the list the UI offers for ticking — not in chain selection.
///
/// Derived from the data rather than keyed on one name, so a game update that
/// unhides one of them fails here instead of quietly changing what a chain
/// may select.
#[test]
fn the_never_unlockable_recipes_were_already_out_of_the_candidate_set() {
    let never_unlockable: Vec<&'static Recipe> = recipe::registry()
        .values()
        .filter(|r| !Availability::Everything.allows(r))
        .collect();
    assert_eq!(never_unlockable.len(), 8, "the closed set pinned by technology_regression.rs");

    for r in never_unlockable {
        assert!(r.hidden, "{} is not hidden, so this test's premise has moved", r.name);
        for result in &r.results {
            assert!(
                candidates_for(&result.name).iter().all(|c| c.name != r.name),
                "{} reached the candidate set for {}",
                r.name,
                result.name
            );
            // …and the gate agrees, end to end.
            assert!(
                available_candidates_for(&result.name, &Availability::Everything)
                    .iter()
                    .all(|c| c.name != r.name)
            );
        }
    }
}

/// Both copper-cable producers are locked: `RecipeLocked`, not
/// `AmbiguousRecipe` — there is nothing to choose between, only something to
/// unlock.
#[test]
fn locked_recipe_errors_with_the_locked_candidates_named() {
    let a = unlocked(&["iron-plate"]);
    match select_recipe("copper-cable", &HashMap::new(), &a) {
        Err(ChainError::RecipeLocked { item, recipes, .. }) => {
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
