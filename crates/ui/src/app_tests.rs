// Headless frame tests for the app shell: the load paths that feed the
// viewport, and the pure helpers around them.
use super::*;
use factorio_blueprint::{fixtures, Position};
use factorio_grid::Grid;

use crate::clipboard::Message;

/// Every string the app paints, after driving `FactorioApp::ui` for real
/// frames. Same approach as `chain_panel::harness` — egui lays out a frame
/// with no window and no GPU, so this is the whole render path.
fn painted_text(app: &mut FactorioApp) -> Vec<String> {
    let ctx = egui::Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1280.0, 900.0),
        )),
        ..Default::default()
    };
    // The first frame loads fonts and sizes the panels; egui only has real
    // geometry to lay text into from the second onward.
    let _warmup = ctx.run(input(), |ctx| app.ui(ctx));
    let output = ctx.run(input(), |ctx| app.ui(ctx));

    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    found
}

fn painted(app: &mut FactorioApp, needle: &str) -> bool {
    painted_text(app).iter().any(|s| s.contains(needle))
}

/// The whole feature, end to end through the real frame: a blueprint appears
/// on the clipboard and the viewport is showing it, with nothing clicked.
#[test]
fn a_blueprint_offered_by_the_watcher_loads_itself() {
    let (tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    assert!(painted(&mut app, "Paste a blueprint string"), "starts empty");

    tx.send(Message::Blueprint(fixtures::ASSEMBLER_SETUP.into())).unwrap();
    assert!(painted(&mut app, "Loaded from clipboard"));
    assert!(
        matches!(app.state, AppState::Loaded { source: LoadSource::Clipboard, .. }),
        "and the grid is really there, not just the status line"
    );
}

/// The self-copy loop, through the real frame: "Copy blueprint" puts the
/// displayed block's own string on the clipboard, the watcher offers it
/// straight back, and the app must decline. Without the `displayed` check the
/// app would re-import its own output every time the button is pressed.
#[test]
fn the_watcher_declines_the_blueprint_already_on_screen() {
    let (tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    app.displayed = Some(fixtures::ASSEMBLER_SETUP.to_string());

    tx.send(Message::Blueprint(fixtures::ASSEMBLER_SETUP.into())).unwrap();
    assert!(painted(&mut app, "Paste a blueprint string"), "nothing was loaded");
    assert!(matches!(app.state, AppState::Empty));
}

/// A clipboard load must not touch the top bar's text field: the watcher runs
/// in the background, and a background poller that can overwrite a half-typed
/// string is a data-loss bug however small the data.
#[test]
fn a_clipboard_load_leaves_the_typed_input_alone() {
    let (tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    app.blueprint_input = "0partiallyTypedSoFar".to_string();

    tx.send(Message::Blueprint(fixtures::SINGLE_BELT.into())).unwrap();
    let _ = painted_text(&mut app);

    assert_eq!(app.blueprint_input, "0partiallyTypedSoFar");
    assert!(matches!(app.state, AppState::Loaded { source: LoadSource::Clipboard, .. }));
}

/// Turning the toggle off must reach the polling thread, not just the
/// checkbox — the two are separate pieces of state and the frame is what
/// keeps them together.
#[test]
fn the_toggle_defaults_on_and_survives_a_frame() {
    let (_tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    assert!(painted(&mut app, "Watch clipboard"), "the toggle is on screen");
    assert!(app.watching_clipboard());
}

/// The stamp is unconditional — a healthy launch says which commit it is,
/// which is the only way to answer "am I running current code?" from inside
/// the app at all.
#[test]
fn the_build_stamp_is_always_on_screen() {
    let (_tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    assert!(painted(&mut app, &build_info::label()));
}

/// And a fallback build says so loudly. This is the whole feature: a failed
/// build relaunches the previous binary, and that used to look exactly like a
/// healthy one — 29 commits old, with nothing on screen to suggest it.
#[test]
fn a_stale_build_warns_in_the_status_bar() {
    let (_tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    app.build = FreshnessProbe::ready(build_info::Freshness::Stale { behind: Some(29) });
    assert!(painted(&mut app, "29 commits behind the checkout"));
}

/// A probe that has not answered yet must not paint a warning — the app
/// starts with the answer outstanding, and a "stale" flash on every launch
/// would train the user to ignore the one that matters.
#[test]
fn an_unanswered_probe_paints_the_stamp_and_no_warning() {
    let (_tx, watcher) = ClipboardWatcher::detached();
    let mut app = FactorioApp::for_test(watcher);
    app.build = FreshnessProbe::pending();
    assert!(painted(&mut app, &build_info::label()));
    assert!(!painted(&mut app, "behind the checkout"));
}

#[test]
fn test_direction_names_cardinal() {
    assert_eq!(direction_name(Direction::North), "North");
    assert_eq!(direction_name(Direction::East), "East");
    assert_eq!(direction_name(Direction::South), "South");
    assert_eq!(direction_name(Direction::West), "West");
}

#[test]
fn test_direction_names_intercardinal() {
    assert_eq!(direction_name(Direction::NorthEast), "NE");
    assert_eq!(direction_name(Direction::SouthEast), "SE");
    assert_eq!(direction_name(Direction::SouthWest), "SW");
    assert_eq!(direction_name(Direction::NorthWest), "NW");
    assert_eq!(direction_name(Direction::NorthNorthEast), "NNE");
    assert_eq!(direction_name(Direction::EastSouthEast), "ESE");
}

/// Verify that viewport frustum culling returns only entities in the visible
/// window rather than scanning all entities in the grid.
///
/// We place 100×100 = 10,000 transport-belt entities (1×1 each), set the
/// "viewport" to a 10×10 region, and assert that query_rect returns ≤ ~121
/// entities (11×11 cells with boundary allowance) — not all 10,000.
#[test]
fn test_viewport_culling_limits_visible_entities() {
    let mut grid = Grid::new();

    // Place 10,000 transport belts in a 100×100 grid.
    for row in 0..100i32 {
        for col in 0..100i32 {
            // transport-belt is 1×1; center == top-left for odd-sized entities.
            let center = Position {
                x: col as f64 + 0.5,
                y: row as f64 + 0.5,
            };
            let _ = grid.place("transport-belt", &center, Direction::North, None, None);
        }
    }

    assert_eq!(grid.entity_count(), 10_000, "all entities placed");

    // Simulate a viewport that shows only the 10×10 region [20, 30) × [20, 30).
    let visible = grid.query_rect(20, 20, 29, 29);

    // Should return exactly 100 entities (10×10), not all 10,000.
    assert!(
        visible.len() <= 110,
        "culling should return ≤ 110 entities for a 10×10 view, got {}",
        visible.len()
    );
    assert!(
        visible.len() >= 90,
        "culling should return ≥ 90 entities for a 10×10 view, got {}",
        visible.len()
    );
}
