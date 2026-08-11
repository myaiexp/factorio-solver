// Render checks for the topology controls added in `layout_controls.rs`:
// long inserter, spine/edge belt counts, ingredients-on side, and the
// optional target-width cap. Split out from `render_tests.rs`, which is
// already close to the project's 300-line file cap, rather than growing it
// past that.
use factorio_solver::layout::Side;

use super::render_tests::{painted_text, solved_green_circuit_panel};
use super::ChainPanel;

/// The topology controls are independent config, like the belt/pole/inserter
/// pickers beside them — on screen before any plan is solved, not gated on a
/// result to act on.
#[test]
fn topology_controls_paint() {
    let mut panel = ChainPanel::new();
    let joined = painted_text(&mut panel).join(" | ");
    assert!(joined.contains("Long inserter"), "{joined}");
    assert!(joined.contains("Spine belts"), "{joined}");
    assert!(joined.contains("Edge belts"), "{joined}");
    assert!(joined.contains("Ingredients on"), "{joined}");
    assert!(joined.contains("Target width"), "{joined}");
}

/// Proves the topology control reaches `LayoutConfig` rather than being
/// decorative: flipping `ingredients_on` swaps which side of a cell shares
/// the spine (both columns draw from it) versus the edge (one column owns
/// it), which changes belt and inserter counts and so the generated entity
/// count, for the same solved plan.
#[test]
fn changing_the_topology_changes_the_block() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();
    let spine_count = match &panel.generated {
        Some(Ok(block)) => block.entity_count,
        other => panic!("expected a generated block, got {other:?}"),
    };

    panel.layout_topology.ingredients_on = Side::Edge;
    panel.generate_block();
    let edge_count = match &panel.generated {
        Some(Ok(block)) => block.entity_count,
        other => panic!("expected a generated block, got {other:?}"),
    };

    assert_ne!(spine_count, edge_count, "flipping ingredients_on must change the generated block");
}

/// The green-circuit block's copper-cable step is capped by its own product
/// lane, not its ingredient — `bound_by` names `copper-cable` for the
/// `copper-cable` recipe. This checks that name reaches the screen as the
/// binding line's own wording, not merely as a recipe name painted
/// elsewhere (the recipe list above already says "Copper cable").
#[test]
fn the_binding_stream_is_shown() {
    let mut panel = solved_green_circuit_panel();
    panel.generate_block();

    let bindings = match &panel.generated {
        Some(Ok(block)) => block.bindings.clone(),
        other => panic!("expected a generated block, got {other:?}"),
    };
    assert!(
        bindings.iter().any(|(recipe, bound_by)| recipe == "copper-cable" && bound_by == "copper-cable"),
        "expected the copper-cable step's binding to be copper-cable, got {bindings:?}"
    );

    let text = painted_text(&mut panel);
    assert!(
        text.iter().any(|t| t.contains("copper-cable") && t.contains("limited by")),
        "no binding line naming copper-cable painted: {text:?}"
    );
}
