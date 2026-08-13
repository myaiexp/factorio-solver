// Tests for the goal/plan types defined directly in `chain/mod.rs`: `Rate`
// conversions, `MachinePolicy` constructors, `ChainGoal` builders, and
// `Availability`. Split out to keep `mod.rs` under the file-length limit.
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
    assert_eq!(g.availability, Availability::Unrestricted);
}

#[test]
fn with_availability_sets_the_field() {
    let a = Availability::Unlocked(HashSet::from(["iron-plate".to_string()]));
    let g = ChainGoal::new("iron-plate", Rate::ItemsPerSec(1.0), &[]).with_availability(a.clone());
    assert_eq!(g.availability, a);
}

#[test]
fn unrestricted_allows_anything() {
    let a = Availability::Unrestricted;
    assert!(a.allows_recipe("not-a-real-recipe"));
    assert!(a.allows_machine("not-a-real-machine"));
}

#[test]
fn unlocked_allows_only_listed_recipes() {
    let a = Availability::Unlocked(HashSet::from(["iron-plate".to_string()]));
    assert!(a.allows_recipe("iron-plate"));
    assert!(!a.allows_recipe("copper-plate"));
}

#[test]
fn allows_machine_is_keyed_on_produced_item_not_recipe_name() {
    // The "assembling-machine-2" recipe produces the "assembling-machine-2"
    // item — same name in this data set, but allows_machine must be
    // checking the *item* on the results list, not string-matching the
    // recipe's own name against `machine`.
    let a = Availability::Unlocked(HashSet::from(["assembling-machine-2".to_string()]));
    assert!(a.allows_machine("assembling-machine-2"));
    assert!(!a.allows_machine("assembling-machine-3"));
}

#[test]
fn allows_machine_false_when_no_unlocked_recipe_produces_it() {
    let a = Availability::Unlocked(HashSet::new());
    assert!(!a.allows_machine("assembling-machine-1"));
}
