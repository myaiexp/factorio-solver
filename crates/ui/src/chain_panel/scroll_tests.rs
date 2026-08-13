// The panel body has to stay reachable when a plan is taller than the window.
//
// Every other render test runs on a deliberately tall screen so the whole
// panel is in view at once; these run at a real window height, where the
// panel's data-dependent sections (one row per step, per input, per bus item)
// push the block generator's controls past the bottom edge.
use egui::vec2;

use super::harness::painted_text_at;
use super::{ChainPanel, MachineChoice, RateUnit};

/// The window the bug was reported at.
const REAL_SCREEN: egui::Vec2 = vec2(1920.0, 1050.0);

/// A window short enough that this plan overflows it by more than a screen,
/// so scrolling to the bottom leaves the top genuinely out of view. At
/// `REAL_SCREEN` the same plan overflows by only a little, and the body
/// scrolls a correspondingly short distance.
const SHORT_SCREEN: egui::Vec2 = vec2(1920.0, 700.0);

/// One belt of chemical science off a five-item bus: six steps, five inputs,
/// which is what it takes to overflow a 1050px-tall panel.
fn chemical_science_panel() -> ChainPanel {
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
    // The assembler recipes, against the Space Age foundry's casting variants
    // — the same choices the panel's own ambiguity buttons offer, made here so
    // the fixture resolves without a human clicking through them.
    for item in ["copper-cable", "iron-gear-wheel", "pipe"] {
        panel.overrides.insert(item.to_string(), item.to_string());
    }
    panel.solve();
    assert!(matches!(panel.result, Some(Ok(_))), "fixture must solve: {:?}", panel.result);
    panel
}

/// The premise of the bug: this plan really is taller than the window, so the
/// generator's controls are off-screen until something scrolls. Guards the
/// test below from silently passing because the fixture got short enough to
/// fit, which would prove nothing about scrolling.
#[test]
fn the_long_chain_overflows_a_real_window() {
    let mut panel = chemical_science_panel();
    let joined = painted_text_at(&mut panel, REAL_SCREEN, 0).join(" | ");

    assert!(joined.contains("Production Chain"), "the top of the panel must be in view: {joined}");
    assert!(
        !joined.contains("Generate"),
        "fixture no longer overflows the window, so it cannot test scrolling: {joined}"
    );
}

/// The regression itself: with the plan solved, scrolling the panel body must
/// bring the Generate button into view. Before the body was a `ScrollArea`
/// the wheel did nothing and the button stayed clipped off the bottom edge,
/// unreachable — a generated blueprint you could never ask for.
#[test]
fn scrolling_brings_generate_into_view() {
    let mut panel = chemical_science_panel();
    let joined = painted_text_at(&mut panel, REAL_SCREEN, 8).join(" | ");

    assert!(joined.contains("Generate"), "Generate is unreachable by scrolling: {joined}");
}

/// Scrolling has to move the body past the window, not merely stretch the
/// panel to fit everything: on a screen this plan overflows by more than its
/// own height, reaching the bottom must carry the heading off the top.
#[test]
fn scrolling_moves_the_panel_body_past_the_window() {
    let mut panel = chemical_science_panel();
    let joined = painted_text_at(&mut panel, SHORT_SCREEN, 8).join(" | ");

    assert!(joined.contains("Generate"), "Generate is unreachable by scrolling: {joined}");
    assert!(
        !joined.contains("Production Chain"),
        "the heading should have scrolled out of view: {joined}"
    );
}
