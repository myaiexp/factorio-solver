// Headless render checks: drive the real `ChainPanel::ui` through a live egui
// frame and read back the text it actually paints.
//
// `build_goal` tests prove the maths reaches the solver; these prove the
// numbers reach the screen. egui needs no window or GPU to lay out a frame,
// so this is the whole render path — widgets, tables, error UI — with no
// compositor involved.
use egui::epaint::Shape;
use egui::{pos2, vec2, Context, RawInput, Rect};
use factorio_blueprint::decode;

use super::{ChainPanel, MachineChoice, RateUnit};

/// Lays out one frame of the panel and returns every string it painted.
/// `pub(super)` so `topology_tests.rs`, a sibling test module split out to
/// keep this file under the project's line cap, can drive the same real
/// render path rather than reimplementing it.
pub(super) fn painted_text(panel: &mut ChainPanel) -> Vec<String> {
    let ctx = Context::default();
    // Tall enough that a solved-and-generated panel's full content — steps
    // table, block-generator controls, and the generated block's own stats —
    // stays inside the `SidePanel`'s clip rect. egui's `Label` skips painting
    // (and so skips adding a `Shape::Text`) for a rect outside the clip rect
    // as a layout-cost optimization, so anything shorter here silently drops
    // the tail of the panel from `output.shapes` with no panic to flag it.
    let input = || RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 2400.0))),
        ..Default::default()
    };

    // Two frames: the first loads fonts and sizes the panel, and egui only
    // has real geometry to lay text into from the second onward.
    let _warmup = ctx.run(input(), |ctx| {
        egui::SidePanel::right("chain_panel").show(ctx, |ui| panel.ui(ui));
    });
    let output = ctx.run(input(), |ctx| {
        egui::SidePanel::right("chain_panel").show(ctx, |ui| panel.ui(ui));
    });

    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    found
}

fn collect(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Text(text) => out.push(text.galley.text().to_string()),
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
        _ => {}
    }
}

fn green_circuit_panel() -> ChainPanel {
    let mut panel = ChainPanel::new();
    panel.product = "electronic-circuit".to_string();
    panel.rate_unit = RateUnit::PerSec;
    panel.rate_value = 45.0;
    panel.available = vec!["iron-plate".to_string(), "copper-plate".to_string()];
    panel.machine = MachineChoice::Named("assembling-machine-2".to_string());
    panel
}

/// The green-circuit goal, with the `copper-cable` override in place so it
/// actually solves instead of stopping at `AmbiguousRecipe` — what the
/// generator tests need, since generation only ever runs against `Ok(plan)`.
/// `pub(super)` for the same reason as `painted_text`: `topology_tests.rs`
/// reuses this fixture rather than duplicating it.
pub(super) fn solved_green_circuit_panel() -> ChainPanel {
    let mut panel = green_circuit_panel();
    panel.overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
    panel.solve();
    assert!(matches!(panel.result, Some(Ok(_))), "fixture must solve: {:?}", panel.result);
    panel
}

/// The layout phase's own refusal case, built directly rather than through
/// `factorio_solver::testsupport` (not available outside the solver crate):
/// a self-consuming recipe (kovarex) that the chain calculator resolves fine
/// but the generator cannot lay out.
fn kovarex_panel() -> ChainPanel {
    let mut panel = ChainPanel::new();
    panel.product = "uranium-235".to_string();
    panel.rate_unit = RateUnit::PerSec;
    panel.rate_value = 1.0;
    panel.available = vec!["uranium-ore".to_string()];
    panel.overrides.insert("uranium-235".to_string(), "kovarex-enrichment-process".to_string());
    panel.overrides.insert("uranium-238".to_string(), "uranium-processing".to_string());
    panel.solve();
    assert!(matches!(panel.result, Some(Ok(_))), "kovarex plan must still solve: {:?}", panel.result);
    panel
}

#[test]
fn a_fresh_panel_renders_its_controls() {
    let mut panel = ChainPanel::new();
    let text = painted_text(&mut panel);
    assert!(text.iter().any(|t| t.contains("Production Chain")), "no heading painted: {text:?}");
    assert!(text.iter().any(|t| t.contains("Solve")), "no Solve button painted: {text:?}");
}

/// The screenshot assertion, minus the screenshot: 45/s green circuits from
/// plates on assembling-machine-2 must show 30 and 45 machines on screen.
#[test]
fn the_green_circuit_plan_paints_30_and_45() {
    let mut panel = green_circuit_panel();
    panel.overrides.insert("copper-cable".to_string(), "copper-cable".to_string());
    panel.solve();

    let text = painted_text(&mut panel);
    let joined = text.join(" | ");

    // Display names, not internal ones — the panel is user-facing.
    assert!(joined.contains("Electronic circuit"), "{joined}");
    assert!(joined.contains("Copper cable"), "{joined}");
    assert!(joined.contains("Assembling machine 2"), "{joined}");

    // The machine counts themselves, as painted.
    assert!(
        text.iter().any(|t| t.trim() == "30" || t.contains("30 (")),
        "no 30-machine row painted: {joined}"
    );
    assert!(
        text.iter().any(|t| t.trim() == "45" || t.contains("45 (")),
        "no 45-machine row painted: {joined}"
    );

    // The bus inputs the plan asks for.
    assert!(joined.contains("Iron plate") || joined.contains("iron-plate"), "{joined}");
}

/// Ambiguity has to be escapable from the panel: without the override the
/// same goal errors, and the panel must paint both candidates as choices.
#[test]
fn an_ambiguous_goal_paints_its_candidates_as_buttons() {
    let mut panel = green_circuit_panel();
    panel.solve();

    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains("copper-cable"), "error text must name the item: {joined}");
    assert!(
        joined.contains("casting-copper-cable"),
        "both candidates must be offered, not just one: {joined}"
    );
}

/// A solve that cannot proceed shows the message rather than an empty panel.
#[test]
fn a_fluid_error_paints_its_remedy() {
    let mut panel = ChainPanel::new();
    panel.product = "plastic-bar".to_string();
    panel.available = vec!["coal".to_string()];
    panel.overrides.insert("plastic-bar".to_string(), "plastic-bar".to_string());
    panel.solve();

    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains("petroleum-gas"), "must name the fluid: {joined}");
    assert!(joined.contains("available"), "must state the fix: {joined}");
}

/// The block generator's own selectors must be on screen even before any
/// plan is solved — they're independent config, not something that appears
/// only once there's a result to act on.
#[test]
fn layout_controls_paint_on_a_fresh_panel() {
    let mut panel = ChainPanel::new();
    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains("Belt tier"), "{joined}");
    assert!(joined.contains("Pole"), "{joined}");
    assert!(joined.contains("Inserter"), "{joined}");
    assert!(joined.contains("Generate"), "no Generate button painted: {joined}");
}

/// End to end: solve, generate, and read the size/entity count back off the
/// screen, with no error text anywhere in the frame.
#[test]
fn a_successful_generate_paints_size_and_entity_count() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();

    let (width, height, entity_count) = match &panel.generated {
        Some(Ok(block)) => (block.width, block.height, block.entity_count),
        other => panic!("expected a generated block, got {other:?}"),
    };

    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains(&format!("{width} x {height}")), "block size not painted: {joined}");
    assert!(joined.contains(&format!("{entity_count}")), "entity count not painted: {joined}");
    assert!(!joined.contains("no belt-fed layout"), "unexpected error text: {joined}");
}

/// The number a human checks before pasting: express belts + fast inserters
/// (the mid-game config `factorio_solver::testsupport::default_cfg` also
/// pins its tests on) puts the green-circuit block at 53x57 tiles, 843
/// entities. A change here is either an intentional layout-phase change or a
/// real regression, not something this test should paper over with a looser
/// assertion.
#[test]
fn the_blue_belt_config_green_circuit_block_matches_the_known_size() {
    let mut panel = solved_green_circuit_panel();
    panel.layout_belt = "express-transport-belt".to_string();
    panel.layout_inserter = "fast-inserter".to_string();
    panel.generate_block();

    match &panel.generated {
        Some(Ok(block)) => {
            assert_eq!((block.width, block.height), (53, 57));
            assert_eq!(block.entity_count, 843);
        }
        other => panic!("expected a generated block, got {other:?}"),
    }
}

/// The blueprint string painted/stored is the one that actually round-trips
/// — asserted on the panel's own field, not on painted text, so a bug that
/// shows one string while copying another cannot pass.
#[test]
fn the_stored_blueprint_string_round_trips_to_the_same_entity_count() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();

    let Some(Ok(block)) = &panel.generated else {
        panic!("expected a generated block, got {:?}", panel.generated);
    };
    assert!(!block.blueprint_string.is_empty());
    assert!(block.blueprint_string.starts_with('0'), "{}", block.blueprint_string);

    let data = decode(&block.blueprint_string).expect("stored string must decode");
    let entities = data.blueprint.expect("a blueprint, not a book").entities;
    assert_eq!(entities.len(), block.entity_count);
}

/// `take_generated_grid` is a one-shot handoff to the viewport: a second
/// call with nothing new generated in between must come back empty.
#[test]
fn take_generated_grid_yields_the_grid_exactly_once() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();

    let expected_count = match &panel.generated {
        Some(Ok(block)) => block.entity_count,
        other => panic!("expected a generated block, got {other:?}"),
    };

    let grid = panel.take_generated_grid().expect("a grid was just generated");
    assert_eq!(grid.entity_count(), expected_count);
    assert!(panel.take_generated_grid().is_none(), "second take must be empty");
}

/// The layout phase's own refusal (a self-looping kovarex step) must reach
/// the screen naming both the recipe and the item, with no blueprint string
/// offered in its place.
#[test]
fn a_layout_failure_paints_the_error_and_no_blueprint_string() {
    let mut panel = kovarex_panel();
    panel.generate_block();

    assert!(matches!(panel.generated, Some(Err(_))), "expected a layout error, got {:?}", panel.generated);

    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains("kovarex-enrichment-process"), "{joined}");
    assert!(joined.contains("uranium-235"), "{joined}");
    assert!(!joined.contains("tiles,"), "no block stats should be painted on failure: {joined}");
}

/// Proves the belt-tier selector actually reaches `LayoutConfig` rather than
/// being decorative. In the columnar topology a slower belt does not simply
/// add more entities the way the old row topology's "more parallel sub-rows"
/// did: it packs fewer machines into each cell, which trades a taller band
/// of a few big cells (fewer repeated spine/edge belt columns, but each
/// column taller — more of `place.rs`'s reserved pole rows baked into it)
/// for a wider band of many small ones (more repeated columns, each
/// shorter). Which trade wins on total entity count depends on the plan's
/// own machine counts and cell-boundary rounding, not on belt speed alone —
/// so this only asserts that the tier reaches the generator at all, not
/// which direction the count moves.
#[test]
fn changing_belt_tier_changes_the_generated_entity_count() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();
    let default_count = match &panel.generated {
        Some(Ok(block)) => block.entity_count,
        other => panic!("expected a generated block, got {other:?}"),
    };

    panel.layout_belt = "express-transport-belt".to_string();
    panel.generate_block();
    let express_count = match &panel.generated {
        Some(Ok(block)) => block.entity_count,
        other => panic!("expected a generated block, got {other:?}"),
    };

    assert_ne!(default_count, express_count, "belt tier must change the generated block");
}
