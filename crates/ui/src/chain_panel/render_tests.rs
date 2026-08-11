// Headless render checks: drive the real `ChainPanel::ui` through a live egui
// frame and read back the text it actually paints.
//
// `build_goal` tests prove the maths reaches the solver; these prove the
// numbers reach the screen. egui needs no window or GPU to lay out a frame,
// so this is the whole render path — widgets, tables, error UI — with no
// compositor involved.
use egui::epaint::Shape;
use egui::{pos2, vec2, Context, RawInput, Rect};

use super::{ChainPanel, MachineChoice, RateUnit};

/// Lays out one frame of the panel and returns every string it painted.
fn painted_text(panel: &mut ChainPanel) -> Vec<String> {
    let ctx = Context::default();
    let input = || RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
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
