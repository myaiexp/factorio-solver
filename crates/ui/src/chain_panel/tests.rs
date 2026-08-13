// Unit tests for the chain panel's own state: goal construction from the
// panel's fields, including the availability the save picker supplies.
use factorio_solver::chain::Availability;

use super::*;

fn panel_with(product: &str, available: &[&str]) -> ChainPanel {
    let mut p = ChainPanel::new();
    p.product = product.to_string();
    p.available = available.iter().map(|s| s.to_string()).collect();
    p
}

#[test]
fn goal_uses_items_per_sec_unit() {
    let mut p = panel_with("electronic-circuit", &[]);
    p.rate_unit = RateUnit::PerSec;
    p.rate_value = 45.0;
    assert_eq!(p.build_goal().rate, Rate::ItemsPerSec(45.0));
}

#[test]
fn goal_uses_items_per_min_unit() {
    let mut p = panel_with("electronic-circuit", &[]);
    p.rate_unit = RateUnit::PerMin;
    p.rate_value = 120.0;
    assert_eq!(p.build_goal().rate, Rate::ItemsPerMin(120.0));
}

#[test]
fn goal_uses_belts_unit_with_tier() {
    let mut p = panel_with("electronic-circuit", &[]);
    p.rate_unit = RateUnit::Belts;
    p.rate_value = 2.0;
    p.belt_tier = "express-transport-belt".to_string();
    assert_eq!(
        p.build_goal().rate,
        Rate::Belts { count: 2, tier: "express-transport-belt".to_string() }
    );
}

#[test]
fn goal_carries_available_list() {
    let p = panel_with("electronic-circuit", &["iron-plate", "copper-plate"]);
    let goal = p.build_goal();
    assert!(goal.available.contains("iron-plate"));
    assert!(goal.available.contains("copper-plate"));
}

#[test]
fn goal_carries_overrides() {
    let mut p = panel_with("electronic-circuit", &[]);
    p.overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
    let goal = p.build_goal();
    assert_eq!(goal.recipe_overrides.get("copper-cable").unwrap(), "copper-cable");
}

#[test]
fn build_goal_is_unrestricted_when_no_save_is_selected() {
    let panel = ChainPanel::default();
    assert_eq!(panel.build_goal().availability, Availability::Unrestricted);
}

/// `build_goal` must read `save_picker.availability` straight through
/// with no re-derivation — proven here by setting the field directly
/// rather than going through a real decode, which `save_picker`'s own
/// tests already cover.
#[test]
fn build_goal_carries_unlocked_availability_once_a_save_is_loaded() {
    let mut p = panel_with("electronic-circuit", &[]);
    let unlocked: std::collections::HashSet<String> =
        ["iron-plate", "copper-plate"].iter().map(|s| s.to_string()).collect();
    p.save_picker.availability = Availability::Unlocked(unlocked.clone());
    assert_eq!(p.build_goal().availability, Availability::Unlocked(unlocked));
}

/// The number a human checks on screen: 45/s electronic circuits from
/// plates, on assembling-machine-2, needs 30 machines making circuits
/// and 45 making the copper cable they consume.
#[test]
fn end_to_end_electronic_circuit_plan_matches_expected_machine_counts() {
    let mut p = panel_with("electronic-circuit", &["iron-plate", "copper-plate"]);
    p.rate_unit = RateUnit::PerSec;
    p.rate_value = 45.0;
    p.machine = MachineChoice::Named("assembling-machine-2".to_string());
    p.overrides.insert("copper-cable".to_string(), "copper-cable".to_string());

    let plan = solve(&p.build_goal()).expect("plan resolves with the override in place");
    let counts: Vec<u32> = plan.steps.iter().map(|s| s.machines_needed).collect();
    assert!(counts.contains(&30), "expected a 30-machine step, got {counts:?}");
    assert!(counts.contains(&45), "expected a 45-machine step, got {counts:?}");
    assert_eq!(plan.steps.len(), 2, "only electronic-circuit and copper-cable should run");
}
