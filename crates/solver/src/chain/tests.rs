// Tests for the goal/plan types defined directly in `chain/mod.rs`: `Rate`
// conversions, `MachinePolicy` constructors, `ChainGoal` builders, and
// `Availability`. Split out to keep `mod.rs` under the file-length limit.
use std::collections::BTreeSet;

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
    assert_eq!(Rate::Belts { count: 1, tier: "transport-belt".into() }.per_sec().unwrap(), 15.0);
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
    let e = ChainError::FluidIngredient { recipe: "plastic-bar".into(), fluid: "petroleum-gas".into() };
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
    assert_eq!(g.availability, Availability::Everything);
}

#[test]
fn with_availability_sets_the_field() {
    let a = Availability::Unlocked(BTreeSet::from(["iron-plate".to_string()]));
    let g = ChainGoal::new("iron-plate", Rate::ItemsPerSec(1.0), &[]).with_availability(a.clone());
    assert_eq!(g.availability, a);
}

/// `Everything` treats an unknown name differently depending on which
/// question is asked. `allows_recipe` looks the name up in the recipe
/// registry first, so a typo or a retired recipe name reads as "not
/// allowed" — the alternative would let either read as buildable.
/// `allows_machine` instead asks "does any recipe produce this item", and a
/// name nothing produces at all (an unknown name included) answers "yes" by
/// the same rule that keeps editor-only prototypes and crash-site wreckage
/// available: missing data is not evidence of a lock.
#[test]
fn everything_treats_an_unknown_recipe_and_an_unknown_machine_differently() {
    let a = Availability::Everything;
    assert!(!a.allows_recipe("not-a-real-recipe"));
    assert!(a.allows_machine("not-a-real-machine"));
}

#[test]
fn unlocked_allows_only_listed_recipes() {
    let a = Availability::Unlocked(BTreeSet::from(["iron-plate".to_string()]));
    assert!(a.allows_recipe("iron-plate"));
    // casting-iron is `enabled: false`, so — unlike a starting recipe — it
    // actually needs to be in the set to be allowed. copper-plate (this
    // fixture's original choice) is `enabled: true` and reads as allowed
    // under any set, which would prove nothing about the `Unlocked` branch.
    assert!(!a.allows_recipe("casting-iron"));
}

#[test]
fn allows_machine_is_keyed_on_produced_item_not_recipe_name() {
    // The "assembling-machine-2" recipe produces the "assembling-machine-2"
    // item — same name in this data set, but allows_machine must be
    // checking the *item* on the results list, not string-matching the
    // recipe's own name against `machine`.
    let a = Availability::Unlocked(BTreeSet::from(["assembling-machine-2".to_string()]));
    assert!(a.allows_machine("assembling-machine-2"));
    assert!(!a.allows_machine("assembling-machine-3"));
}

/// `assembling-machine-1`'s own crafting recipe is `enabled: false` — like
/// everything else built in an assembler, it is gated behind the
/// "automation" technology, not free at game start. But `allows_machine`
/// scans every recipe that produces the item, not just the "intended" one,
/// and `assembling-machine-2-recycling` also lists a fractional
/// `assembling-machine-1` among its results — a partial refund — and is
/// itself `enabled: true`. So the machine reads as available under even the
/// emptiest `Unlocked` set, correctly per `allows_machine`'s own contract:
/// a recycling recipe is a real way to obtain an item, and machine
/// availability does not filter it out the way `candidates_for` does.
/// `assembling-machine-3` has no second producer, so it stays the negative
/// case.
#[test]
fn allows_machine_true_via_any_producing_recipe_even_a_recycling_one() {
    let a = Availability::Unlocked(BTreeSet::new());
    assert!(a.allows_machine("assembling-machine-1"));
    assert!(!a.allows_machine("assembling-machine-3"));
}
