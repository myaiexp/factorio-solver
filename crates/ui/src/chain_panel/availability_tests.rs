// Tests for the availability gate's own panel section: mode seeding, edit
// persistence across mode switches, the tick/untick-all buttons, product
// picker filtering, painting, and the end-to-end reported case.
use factorio_solver::availability::Availability;
use factorio_solver::recipe;

use super::availability_controls::{tick_all_matching, untick_all_matching, AvailabilityMode};
use super::harness::painted_text;
use super::logic::filtered_recipes;
use super::{controls, ChainPanel, MachineChoice, RateUnit};

#[test]
fn switching_to_only_available_seeds_the_set_from_everything() {
    let mut panel = ChainPanel::new();
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);

    // The switch alone must change no solve result, so this asserts the
    // property directly — every recipe's allowed-ness matches `Everything`
    // — rather than comparing set sizes, which could coincidentally agree.
    let seeded = panel.availability();
    for r in recipe::registry().values() {
        assert_eq!(seeded.allows(r), Availability::Everything.allows(r), "{}", r.name);
    }
}

#[test]
fn returning_to_only_available_does_not_discard_edits() {
    let mut panel = ChainPanel::new();
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);

    let casting = recipe::get("casting-copper-cable").expect("fixture recipe exists");
    assert!(panel.availability().allows(casting), "starts allowed: seeded from Everything");
    untick_all_matching(&mut panel, &[casting]);
    assert!(!panel.availability().allows(casting), "test premise: the untick took effect");

    panel.set_availability_mode(AvailabilityMode::Everything);
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);

    assert!(!panel.availability().allows(casting), "the edit must survive the round trip");
}

#[test]
fn untick_all_matching_removes_exactly_the_searched_recipes() {
    let mut panel = ChainPanel::new();
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
    panel.availability_query = "casting".to_string();

    let registry = recipe::registry();
    let matches = filtered_recipes(registry.values(), &panel.availability_query, false);
    assert_eq!(matches.len(), 9, "test premise: exactly the nine casting-* recipes match");

    untick_all_matching(&mut panel, &matches);

    let after_untick = panel.availability();
    for r in &matches {
        assert!(!after_untick.allows(r), "{} should be locked after untick-all", r.name);
    }
    for control_item in ["iron-plate", "electronic-circuit"] {
        let control_recipe = recipe::get(control_item).expect("fixture recipe exists");
        assert!(
            after_untick.allows(control_recipe),
            "{control_item} is outside the 'casting' query and must be untouched"
        );
    }

    tick_all_matching(&mut panel, &matches);
    let after_tick = panel.availability();
    for r in &matches {
        assert!(after_tick.allows(r), "{} should be restored after tick-all", r.name);
    }
}

#[test]
fn the_section_paints_its_controls_and_count() {
    let mut panel = ChainPanel::new();
    let text = painted_text(&mut panel);
    assert!(text.iter().any(|t| t.contains("Available recipes")), "{text:?}");
    assert!(text.iter().any(|t| t.contains("Everything")), "{text:?}");
    assert!(text.iter().any(|t| t.contains("Only what I can build")), "{text:?}");
    assert!(
        text.iter().any(|t| t.contains("of") && t.contains("available")),
        "no count line painted: {text:?}"
    );
    assert!(!text.iter().any(|t| t.contains("Untick all")), "buttons must be hidden in Everything mode");

    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
    let text = painted_text(&mut panel);
    assert!(text.iter().any(|t| t.contains("Tick all")), "{text:?}");
    assert!(text.iter().any(|t| t.contains("Untick all")), "{text:?}");
}

#[test]
fn build_goal_carries_the_availability() {
    let mut panel = ChainPanel::new();
    panel.product = "electronic-circuit".to_string();
    assert_eq!(panel.build_goal().availability, Availability::Everything);

    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
    let casting = recipe::get("casting-copper-cable").expect("fixture recipe exists");
    untick_all_matching(&mut panel, &[casting]);

    let goal_availability = panel.build_goal().availability;
    assert!(matches!(goal_availability, Availability::Unlocked(_)));
    for r in recipe::registry().values() {
        assert_eq!(goal_availability.allows(r), panel.availability().allows(r), "{}", r.name);
    }
}

#[test]
fn the_product_picker_hides_unavailable_recipes() {
    let mut panel = ChainPanel::new();
    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);

    let registry = recipe::registry();
    let casting_matches = filtered_recipes(registry.values(), "casting", false);
    untick_all_matching(&mut panel, &casting_matches);

    let picker_names: Vec<&str> = controls::picker_matches(&panel).iter().map(|r| r.name.as_str()).collect();
    assert!(
        !picker_names.contains(&"casting-copper-cable"),
        "an unbuildable recipe must not be offered as a goal: {picker_names:?}"
    );
    assert!(picker_names.contains(&"electronic-circuit"), "{picker_names:?}");
}

/// The payoff, and the one that proves the whole feature: what
/// `scroll_tests.rs`'s `chemical_science_panel` fixture needs three
/// hand-entered overrides for today (`copper-cable`, `iron-gear-wheel`,
/// `pipe`) instead needs one search-and-untick here, with `overrides` left
/// empty — the reported complaint, solved in two clicks.
#[test]
fn the_reported_case_solves_with_no_overrides() {
    let mut panel = ChainPanel::new();
    panel.product = "chemical-science-pack".to_string();
    panel.rate_unit = RateUnit::Belts;
    panel.rate_value = 1.0;
    panel.belt_tier = "transport-belt".to_string();
    panel.available = ["sulfur", "steel-plate", "plastic-bar", "copper-plate", "iron-plate"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    panel.machine = MachineChoice::Fastest;

    panel.set_availability_mode(AvailabilityMode::OnlyAvailable);
    panel.availability_query = "casting".to_string();
    let matches = filtered_recipes(recipe::registry().values(), &panel.availability_query, false);
    untick_all_matching(&mut panel, &matches);

    panel.solve();

    let plan = match &panel.result {
        Some(Ok(plan)) => plan,
        other => panic!("expected the plan to solve with no overrides, got {other:?}"),
    };
    let mut names: Vec<&str> = plan.steps.iter().map(|s| s.recipe.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "advanced-circuit",
            "chemical-science-pack",
            "copper-cable",
            "electronic-circuit",
            "engine-unit",
            "iron-gear-wheel",
            "pipe",
        ]
    );
}

// ── Reloading a save that changed on disk ──────────────────────────────

/// The auto-reload, through the real frame rather than through `poll`: the
/// game writes a new save while the panel is open, and the next frame the
/// panel draws already knows about the newly researched recipe. This is the
/// wiring the unit tests cannot see — a `poll` nobody calls passes every one
/// of them.
#[test]
fn a_save_rewritten_while_the_panel_is_open_is_picked_up_by_the_next_frame() {
    use super::save_picker::{fixture_save, write_at};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    write_at(&path, &fixture_save(&[]), 1_700_000_000);

    let mut panel = ChainPanel::new();
    panel.adopt_save(&path);
    assert_eq!(
        panel.available_recipes.as_ref().map(|set| set.contains("beacon")),
        Some(false),
        "the imported save has not researched it"
    );

    write_at(&path, &fixture_save(&["beacon"]), 1_700_000_100);
    let _ = painted_text(&mut panel);

    assert!(
        panel.available_recipes.as_ref().is_some_and(|set| set.contains("beacon")),
        "the frame did not pick the change up"
    );
}

/// And when adopting it would cost hand edits, the frame offers the choice
/// instead of taking it.
#[test]
fn a_changed_save_that_would_discard_edits_paints_a_reload_button() {
    use super::save_picker::{fixture_save, write_at};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    write_at(&path, &fixture_save(&[]), 1_700_000_000);

    let mut panel = ChainPanel::new();
    panel.adopt_save(&path);
    // The user unticks one of the imported recipes.
    let removed = panel.available_recipes.as_ref().unwrap().iter().next().cloned().unwrap();
    panel.available_recipes.as_mut().unwrap().remove(&removed);

    write_at(&path, &fixture_save(&["beacon"]), 1_700_000_100);
    let joined = painted_text(&mut panel).join(" | ");

    assert!(joined.contains("Save changed on disk"), "{joined}");
    assert!(joined.contains("Reload"), "no Reload button painted: {joined}");
    assert!(
        !panel.available_recipes.as_ref().unwrap().contains("beacon"),
        "and nothing was adopted behind the user's back"
    );
}

