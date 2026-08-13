// Tests for persist.rs: serde round-trip, leniency (missing/unknown
// fields), the two restore-time repairs (ghost recipe filtering,
// OnlyAvailable re-seeding), and that derived state stays out of the blob
// while Default tracks a fresh panel.
//
// Round-tripped through `serde_json` rather than eframe's own RON storage —
// JSON is readable in assertions and the leniency properties under test
// (missing field, unknown field) are a serde-derive property, not a format
// one, so they hold for RON too.
use std::collections::BTreeSet;

use factorio_solver::availability::Availability;
use factorio_solver::layout::{CellTopology, Side};
use factorio_solver::recipe;

use super::PanelState;
use crate::chain_panel::{AvailabilityMode, ChainPanel, MachineChoice, RateUnit};

#[test]
fn a_captured_panel_round_trips_through_serde() {
    let mut panel = ChainPanel::new();
    panel.product = "electronic-circuit".to_string();
    panel.rate_value = 45.0;
    panel.rate_unit = RateUnit::PerMin;
    panel.belt_tier = "fast-transport-belt".to_string();
    panel.available = vec!["iron-plate".to_string(), "copper-plate".to_string()];
    panel.machine = MachineChoice::Named("assembling-machine-2".to_string());
    panel.show_hidden_recycling = true;
    panel.overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
    panel.layout_belt = "fast-transport-belt".to_string();
    panel.layout_pole = "substation".to_string();
    panel.layout_inserter = "fast-inserter".to_string();
    // `layout_*` fields are plain strings here — `LayoutConfig::resolve` is
    // what validates them against the prototype registry, at generate time,
    // not this serde round-trip. Any value distinguishable from
    // `ChainPanel::new()`'s "long-handed-inserter" default is enough to
    // prove the field was actually captured rather than silently defaulted.
    panel.layout_long_inserter = "super-long-handed-inserter".to_string();
    panel.layout_topology = CellTopology {
        spine_belts: 1,
        edge_belts: 2,
        ingredients_on: Side::Edge,
        target_width: Some(80),
    };
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
    panel.available_recipes =
        Some(BTreeSet::from(["iron-plate".to_string(), "electronic-circuit".to_string()]));

    let json = serde_json::to_string(&PanelState::capture(&panel)).expect("serializes");
    let restored: PanelState = serde_json::from_str(&json).expect("deserializes");
    let mut fresh = ChainPanel::new();
    restored.apply(&mut fresh);

    assert_eq!(fresh.product, "electronic-circuit");
    assert_eq!(fresh.rate_value, 45.0);
    assert_eq!(fresh.rate_unit, RateUnit::PerMin);
    assert_eq!(fresh.belt_tier, "fast-transport-belt");
    assert_eq!(fresh.available, vec!["iron-plate".to_string(), "copper-plate".to_string()]);
    assert_eq!(fresh.machine, MachineChoice::Named("assembling-machine-2".to_string()));
    assert!(fresh.show_hidden_recycling);
    assert_eq!(fresh.overrides.get("copper-cable"), Some(&"copper-cable".to_string()));
    assert_eq!(fresh.layout_belt, "fast-transport-belt");
    assert_eq!(fresh.layout_pole, "substation");
    assert_eq!(fresh.layout_inserter, "fast-inserter");
    assert_eq!(fresh.layout_long_inserter, "super-long-handed-inserter");
    assert_eq!(
        fresh.layout_topology,
        CellTopology {
            spine_belts: 1,
            edge_belts: 2,
            ingredients_on: Side::Edge,
            target_width: Some(80)
        }
    );
    assert_eq!(fresh.availability_mode, AvailabilityMode::OnlyAvailable);
    assert_eq!(
        fresh.available_recipes,
        Some(BTreeSet::from(["iron-plate".to_string(), "electronic-circuit".to_string()]))
    );
}

#[test]
fn a_blob_missing_fields_restores_those_fields_to_defaults() {
    let restored: PanelState =
        serde_json::from_str(r#"{"product":"iron-plate"}"#).expect("deserializes");
    let mut panel = ChainPanel::new();
    restored.apply(&mut panel);

    let fresh = ChainPanel::new();
    assert_eq!(panel.product, "iron-plate");
    assert_eq!(panel.rate_value, fresh.rate_value, "must fall back to the fresh-panel default");
    assert_eq!(panel.belt_tier, fresh.belt_tier, "must fall back to the fresh-panel default");
    assert_eq!(
        panel.layout_topology, fresh.layout_topology,
        "must fall back to the fresh-panel default"
    );
}

#[test]
fn an_unknown_field_in_the_blob_is_ignored() {
    let result: Result<PanelState, _> =
        serde_json::from_str(r#"{"product":"iron-plate","retired_setting":42}"#);
    let state = result.expect("an unrecognized field must not fail deserialization");
    assert_eq!(state.product, "iron-plate");
}

#[test]
fn recipe_names_that_no_longer_exist_are_dropped_on_apply() {
    let state = PanelState {
        availability_mode: AvailabilityMode::OnlyAvailable,
        available_recipes: Some(BTreeSet::from([
            "iron-plate".to_string(),
            "a-recipe-that-was-retired".to_string(),
        ])),
        ..PanelState::default()
    };
    let mut panel = ChainPanel::new();
    state.apply(&mut panel);

    let set = panel.available_recipes.expect("a set was restored, not re-seeded");
    assert!(set.contains("iron-plate"), "a real recipe name must survive");
    assert!(!set.contains("a-recipe-that-was-retired"), "a ghost recipe name must be dropped");
}

#[test]
fn a_restored_only_available_mode_with_no_set_is_seeded() {
    let state = PanelState { availability_mode: AvailabilityMode::OnlyAvailable, ..PanelState::default() };
    let mut panel = ChainPanel::new();
    state.apply(&mut panel);

    // Mirrors `availability_tests.rs`'s
    // `switching_to_only_available_seeds_the_set_from_everything`: the
    // property under test is "allows what Everything allows" over the whole
    // registry, not a set size that could coincidentally agree. Read through
    // `build_goal()` rather than the private `ChainPanel::availability()`
    // that test uses — this module is a sibling of `chain_panel`, not a
    // descendant, so only the public accessor is reachable here.
    let seeded = panel.build_goal().availability;
    for r in recipe::registry().values() {
        assert_eq!(seeded.allows(r), Availability::Everything.allows(r), "{}", r.name);
    }
}

#[test]
fn derived_state_is_not_persisted() {
    let panel = ChainPanel::new();
    let json = serde_json::to_string(&PanelState::capture(&panel)).expect("serializes");
    for absent in ["result", "generated", "pending_grid", "available_input", "availability_query"] {
        assert!(!json.contains(absent), "{absent} leaked into the persisted blob: {json}");
    }
}

#[test]
fn defaults_match_a_fresh_panel() {
    let default_json = serde_json::to_string(&PanelState::default()).expect("serializes");
    let fresh_json =
        serde_json::to_string(&PanelState::capture(&ChainPanel::new())).expect("serializes");
    assert_eq!(default_json, fresh_json);
}

// ── App-level settings ───────────────────────────────────────────────────

use super::AppSettings;

#[test]
fn watching_defaults_on() {
    assert!(AppSettings::default().watch_clipboard());
}

#[test]
fn app_settings_round_trip_through_serde() {
    let json = r#"{"watch_clipboard":false}"#;
    let restored: AppSettings = serde_json::from_str(json).expect("parses");
    assert!(!restored.watch_clipboard(), "a deliberate off must survive a restart");
    assert_eq!(serde_json::to_string(&restored).expect("serializes"), json);
}

/// Same leniency contract as `PanelState`: an old blob missing a setting, and
/// a blob carrying one this version dropped, must both restore rather than
/// wipe. Adding a setting later cannot cost the user their saved ones.
#[test]
fn app_settings_tolerate_missing_and_unknown_fields() {
    let empty: AppSettings = serde_json::from_str("{}").expect("an empty blob restores");
    assert_eq!(empty, AppSettings::default());

    let future: AppSettings =
        serde_json::from_str(r#"{"watch_clipboard":false,"a_setting_from_2027":7}"#)
            .expect("an unknown field is ignored, not an error");
    assert!(!future.watch_clipboard());
}
