// Draws one entity into the viewport at the current level of detail.

use egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use factorio_grid::category::EntityCategory;
use factorio_grid::prototype::lookup;

use crate::colors::CategoryStyle;
use crate::icons::IconCache;
use crate::lod::LodLevel;

/// Fraction of the entity's shorter side the icon occupies, leaving the
/// category colour visible as a frame around it.
const ICON_INSET: f32 = 0.8;

/// Everything the per-entity draw needs that does not vary between entities.
/// Bundled so `render_viewport` passes one value per frame rather than eight
/// arguments per entity.
pub struct EntityPainter<'a> {
    pub painter: &'a Painter,
    pub ctx: &'a egui::Context,
    pub icons: &'a mut IconCache,
    pub lod: LodLevel,
    pub zoom: f32,
    pub border_stroke: Stroke,
}

impl EntityPainter<'_> {
    /// Draws one entity. `Medium` and `Minimal` are deliberately unchanged by
    /// icon support: at those zooms entities are a handful of pixels wide, and
    /// keeping them flat colour is what stops large blueprints from regressing.
    pub fn draw(&mut self, rect: Rect, prototype_name: &str) {
        let category = EntityCategory::from_prototype_name(prototype_name);

        match self.lod {
            // Full detail (zoom >= 16 px/cell): coloured rect + dark border,
            // then the real game icon — or the label character when no icon is
            // available, which is exactly what this drew before icons existed.
            LodLevel::Full => {
                self.painter.rect_filled(rect, 0.0, category.color());
                self.painter.rect_stroke(rect, 0.0, self.border_stroke, StrokeKind::Outside);
                if !self.draw_icon(rect, prototype_name) {
                    self.draw_label(rect, category);
                }
            }
            // Medium detail (4 <= zoom < 16): coloured rect only — no border,
            // no label, no icon. Skips two draw calls per entity vs Full.
            LodLevel::Medium => {
                self.painter.rect_filled(rect, 0.0, category.color());
            }
            // Minimal detail (zoom < 4): single muted filled rect. At this zoom
            // entities are < 4 px wide; individual colour accuracy is
            // imperceptible, so we halve each channel to blend quietly into the
            // dark background and cut per-entity overdraw.
            LodLevel::Minimal => {
                let c = category.color();
                let muted = Color32::from_rgb(c.r() / 2, c.g() / 2, c.b() / 2);
                self.painter.rect_filled(rect, 0.0, muted);
            }
        }
    }

    /// Returns false when there is no icon to draw, so the caller can fall back
    /// to the label character. Unknown prototypes, a missing install and decode
    /// failures all land here.
    fn draw_icon(&mut self, rect: Rect, prototype_name: &str) -> bool {
        let Some(proto) = lookup(prototype_name) else { return false };
        let Some(texture) = self.icons.get(self.ctx, proto) else { return false };

        let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        self.painter.image(texture.id(), icon_rect(rect), uv, Color32::WHITE);
        true
    }

    fn draw_label(&self, rect: Rect, category: EntityCategory) {
        let font_size = (self.zoom * 0.5).clamp(8.0, 40.0);
        self.painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            category.label_char().to_string(),
            FontId::monospace(font_size),
            Color32::WHITE,
        );
    }
}

/// Where an icon is drawn inside an entity's footprint. Icons are square, so a
/// 2x4 recycler must not stretch one into a rectangle — the drawn size follows
/// the shorter side, leaving the category colour as a frame.
fn icon_rect(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height()) * ICON_INSET;
    Rect::from_center_size(rect.center(), Vec2::splat(side))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The footprint of a 2x4 recycler at 16 px/cell. The icon must stay square
    /// and centred rather than stretching to fill it.
    #[test]
    fn icon_rect_stays_square_and_centred_on_a_non_square_entity() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(32.0, 64.0));
        let icon = icon_rect(rect);

        assert!((icon.width() - icon.height()).abs() < 0.01, "icon must stay square");
        assert!((icon.center() - rect.center()).length() < 0.01, "icon must stay centred");
        assert!(icon.width() < rect.width(), "icon must leave the colour frame visible");
        assert!(rect.contains_rect(icon), "icon must not overflow the entity");
    }

    #[test]
    fn icon_rect_scales_with_zoom() {
        let small = icon_rect(Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0)));
        let large = icon_rect(Rect::from_min_size(Pos2::ZERO, Vec2::splat(64.0)));
        assert!(large.width() > small.width());
        assert!((large.width() / small.width() - 4.0).abs() < 0.01);
    }

    #[test]
    fn unknown_prototypes_fall_back_rather_than_drawing_an_icon() {
        // The fallback path must stay reachable: blueprint import keeps unknown
        // entities out of the grid today, but a prototype the registry lacks
        // must render as a label, not vanish.
        assert!(lookup("modded-thing-that-does-not-exist").is_none());
    }
}
