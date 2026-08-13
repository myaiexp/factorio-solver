// Headless render harness: drives the real `ChainPanel::ui` through live egui
// frames and reads back the text it actually paints.
//
// egui needs no window or GPU to lay out a frame, so this is the whole render
// path — widgets, tables, error UI — with no compositor involved. Shared by
// `render_tests`, `topology_tests` and `scroll_tests`.
use egui::epaint::Shape;
use egui::{pos2, vec2, Context, Event, Modifiers, MouseWheelUnit, RawInput, Rect, Vec2};

use super::ChainPanel;

/// Tall enough that a solved-and-generated panel's whole content — steps
/// table, block-generator controls, and the generated block's own stats — is
/// in view at once. Tests that only ask *what* the panel paints use this so
/// they never have to scroll first: egui adds no `Shape::Text` for anything
/// out of view, so a shorter screen silently drops the panel's tail with no
/// panic to flag it.
const TALL_SCREEN: Vec2 = vec2(1280.0, 2400.0);

/// One frame's worth of wheel travel, in points. Large enough that a handful
/// of frames reaches the end of any panel these tests build; egui clamps the
/// offset at the bottom, so overshooting costs nothing.
const SCROLL_STEP: f32 = 400.0;

/// Every string the panel paints, with its whole content in view.
pub(super) fn painted_text(panel: &mut ChainPanel) -> Vec<String> {
    painted_text_at(panel, TALL_SCREEN, 0)
}

/// Every string the panel paints on a `screen`-sized display, after scrolling
/// the panel body down for `scroll_frames` frames.
pub(super) fn painted_text_at(
    panel: &mut ChainPanel,
    screen: Vec2,
    scroll_frames: u32,
) -> Vec<String> {
    let ctx = Context::default();
    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), screen);

    // Low in the right-hand panel: inside the panel body, and clear of the
    // recipe picker's own bounded `ScrollArea` up at the top, which would
    // otherwise swallow the wheel events before the body ever saw them.
    let pointer = pos2(screen.x - 40.0, screen.y - 60.0);

    let frame = |scrolling: bool| RawInput {
        screen_rect: Some(screen_rect),
        events: if scrolling {
            vec![
                Event::PointerMoved(pointer),
                Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: vec2(0.0, -SCROLL_STEP),
                    modifiers: Modifiers::default(),
                },
            ]
        } else {
            vec![Event::PointerMoved(pointer)]
        },
        ..Default::default()
    };

    let mut run = |input: RawInput| {
        ctx.run(input, |ctx| {
            egui::SidePanel::right("chain_panel").show(ctx, |ui| panel.ui(ui));
        })
    };

    // The first frame loads fonts and sizes the panel; egui only has real
    // geometry to lay text into from the second onward.
    let _warmup = run(frame(false));
    for _ in 0..scroll_frames {
        let _scrolling = run(frame(true));
    }
    let output = run(frame(false));

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
