/// Camera transform for mapping between world (grid) coordinates and screen pixels.
///
/// Pure `f32` math — no egui dependency. The coordinate system places world
/// origin at the screen center by default, with `zoom` controlling the number
/// of pixels per grid cell.
#[derive(Debug, Clone)]
pub struct ViewportTransform {
    /// World coordinate at the center of the screen.
    pub center: (f32, f32),
    /// Pixels per grid cell.
    pub zoom: f32,
}

impl ViewportTransform {
    /// Create a default viewport centered at world origin with 32 px/cell zoom.
    pub fn new() -> Self {
        Self {
            center: (0.0, 0.0),
            zoom: 32.0,
        }
    }

    /// Convert a world coordinate to a screen pixel position.
    ///
    /// `screen = (world - center) * zoom + screen_size / 2`
    pub fn world_to_screen(&self, world: (f32, f32), screen_size: (f32, f32)) -> (f32, f32) {
        (
            (world.0 - self.center.0) * self.zoom + screen_size.0 / 2.0,
            (world.1 - self.center.1) * self.zoom + screen_size.1 / 2.0,
        )
    }

    /// Convert a screen pixel position to a world coordinate (inverse of
    /// [`world_to_screen`](Self::world_to_screen)).
    pub fn screen_to_world(&self, screen: (f32, f32), screen_size: (f32, f32)) -> (f32, f32) {
        (
            (screen.0 - screen_size.0 / 2.0) / self.zoom + self.center.0,
            (screen.1 - screen_size.1 / 2.0) / self.zoom + self.center.1,
        )
    }

    /// Return the visible world rectangle as `(min_x, min_y, max_x, max_y)`.
    pub fn visible_world_rect(&self, screen_size: (f32, f32)) -> (f32, f32, f32, f32) {
        let min = self.screen_to_world((0.0, 0.0), screen_size);
        let max = self.screen_to_world(screen_size, screen_size);
        (min.0, min.1, max.0, max.1)
    }

    /// Zoom centered on a screen point so that the world coordinate under the
    /// cursor remains fixed after the zoom.
    pub fn zoom_at(&mut self, screen_point: (f32, f32), screen_size: (f32, f32), factor: f32) {
        let world_before = self.screen_to_world(screen_point, screen_size);
        self.zoom *= factor;
        let world_after = self.screen_to_world(screen_point, screen_size);
        // Shift center so the world point under the cursor stays put.
        self.center.0 += world_before.0 - world_after.0;
        self.center.1 += world_before.1 - world_after.1;
    }

    /// Pan the view by a screen-space delta. Dragging right moves the view
    /// left (the world slides right), so the center shifts by `-delta / zoom`.
    pub fn pan(&mut self, screen_delta: (f32, f32)) {
        self.center.0 -= screen_delta.0 / self.zoom;
        self.center.1 -= screen_delta.1 / self.zoom;
    }

    /// Center and zoom the viewport to fit a world-space bounding box with
    /// `padding` cells of margin on each side.
    pub fn fit_to_bounds(
        &mut self,
        min: (f32, f32),
        max: (f32, f32),
        screen_size: (f32, f32),
        padding: f32,
    ) {
        let world_w = (max.0 - min.0) + 2.0 * padding;
        let world_h = (max.1 - min.1) + 2.0 * padding;

        self.center.0 = (min.0 + max.0) / 2.0;
        self.center.1 = (min.1 + max.1) / 2.0;

        // Choose the zoom that fits both axes.
        let zoom_x = screen_size.0 / world_w;
        let zoom_y = screen_size.1 / world_h;
        self.zoom = zoom_x.min(zoom_y);
        // Enforce the same zoom bounds as the scroll handler in app.rs so that
        // fitting to a large or zero-size blueprint cannot produce out-of-range
        // zoom values (e.g. ~0.38 for a 10 000-cell blueprint, or inf/NaN for
        // a zero-size one) that would later cause grid-line iteration to blow up.
        self.zoom = self.zoom.clamp(2.0, 200.0);
    }
}

impl Default for ViewportTransform {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_screen_origin() {
        let vp = ViewportTransform::new();
        let screen = vp.world_to_screen((0.0, 0.0), (800.0, 600.0));
        assert_eq!(screen, (400.0, 300.0));
    }

    #[test]
    fn test_roundtrip_world_screen() {
        let vp = ViewportTransform {
            center: (5.0, 3.0),
            zoom: 48.0,
        };
        let world = (7.5, -2.0);
        let screen = vp.world_to_screen(world, (1280.0, 800.0));
        let back = vp.screen_to_world(screen, (1280.0, 800.0));
        assert!((back.0 - world.0).abs() < 1e-4);
        assert!((back.1 - world.1).abs() < 1e-4);
    }

    #[test]
    fn test_visible_rect_shrinks_with_zoom() {
        let mut vp = ViewportTransform::new();
        let rect1 = vp.visible_world_rect((800.0, 600.0));
        vp.zoom *= 2.0;
        let rect2 = vp.visible_world_rect((800.0, 600.0));
        let width1 = rect1.2 - rect1.0;
        let width2 = rect2.2 - rect2.0;
        assert!((width2 - width1 / 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_zoom_at_preserves_world_point() {
        let mut vp = ViewportTransform {
            center: (10.0, 10.0),
            zoom: 32.0,
        };
        let screen_size = (800.0, 600.0);
        let mouse = (200.0, 150.0);
        let world_before = vp.screen_to_world(mouse, screen_size);
        vp.zoom_at(mouse, screen_size, 1.5);
        let world_after = vp.screen_to_world(mouse, screen_size);
        assert!((world_after.0 - world_before.0).abs() < 1e-3);
        assert!((world_after.1 - world_before.1).abs() < 1e-3);
    }

    #[test]
    fn test_pan_shifts_center() {
        let mut vp = ViewportTransform {
            center: (0.0, 0.0),
            zoom: 32.0,
        };
        vp.pan((32.0, 0.0));
        assert!((vp.center.0 - (-1.0)).abs() < 1e-4);
    }

    #[test]
    fn test_fit_to_bounds_contains_rect() {
        let mut vp = ViewportTransform::new();
        vp.fit_to_bounds((0.0, 0.0), (20.0, 15.0), (800.0, 600.0), 2.0);
        let visible = vp.visible_world_rect((800.0, 600.0));
        assert!(visible.0 <= 0.0 && visible.1 <= 0.0);
        assert!(visible.2 >= 20.0 && visible.3 >= 15.0);
    }

    /// A zero-size bounding box (min == max) used to produce inf or NaN zoom
    /// (screen_px / 0.0 = inf). After the clamp it must stay in [2.0, 200.0].
    #[test]
    fn test_fit_to_bounds_clamps_zoom_zero_size() {
        let mut vp = ViewportTransform::new();
        vp.fit_to_bounds((5.0, 5.0), (5.0, 5.0), (800.0, 600.0), 0.0);
        assert!(
            vp.zoom.is_finite(),
            "zoom must be finite after zero-size fit"
        );
        assert!(
            (2.0..=200.0).contains(&vp.zoom),
            "zoom {} out of [2.0, 200.0]",
            vp.zoom
        );
    }

    /// A very large blueprint (10 000 × 10 000) would produce zoom ≈ 0.38
    /// without clamping. After the clamp zoom must be >= 2.0.
    #[test]
    fn test_fit_to_bounds_large_blueprint_clamps_zoom() {
        let mut vp = ViewportTransform::new();
        vp.fit_to_bounds((0.0, 0.0), (10_000.0, 10_000.0), (800.0, 600.0), 0.0);
        assert!(
            vp.zoom >= 2.0,
            "zoom {} should be >= 2.0 after large-blueprint fit",
            vp.zoom
        );
    }
}
