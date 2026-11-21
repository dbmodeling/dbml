//! Pure geometry and hit-testing helpers for the diagram canvas.

/// Returns `true` if point `p` (in world coordinates) lies within `tolerance` pixels
/// of any segment in `pts`.
#[must_use]
pub fn point_hits_polyline(p: (f32, f32), pts: &[layout::Pos], tolerance: f32) -> bool {
    let tol_sq = tolerance * tolerance;
    pts.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 0.001 {
            0.0
        } else {
            ((p.0 - a.x) * dx + (p.1 - a.y) * dy) / l2
        }
        .clamp(0.0, 1.0);
        let (cx, cy) = (a.x + t * dx, a.y + t * dy);
        let dist_sq = (p.0 - cx).powi(2) + (p.1 - cy).powi(2);
        dist_sq <= tol_sq
    })
}

/// Testa a curva cúbica por segmentos em coordenadas de mundo.
pub fn point_hits_curve(p: (f32, f32), curve: &[layout::Pos; 4], tolerance: f32) -> bool {
    let mut points = Vec::with_capacity(65);
    for i in 0..=64 {
        let t = i as f32 / 64.0;
        let u = 1.0 - t;
        let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        points.push(layout::Pos {
            x: curve.iter().zip(weights).map(|(p, w)| p.x * w).sum(),
            y: curve.iter().zip(weights).map(|(p, w)| p.y * w).sum(),
        });
    }
    point_hits_polyline(p, &points, tolerance)
}

/// Minimap dimensions in screen pixels (shared with `packages/canvas-web/src/minimap.ts`).
pub const MINIMAP_W: f32 = 210.0;
/// Minimap height in screen pixels.
pub const MINIMAP_H: f32 = 136.0;
/// Inner padding inside the minimap frame.
pub const MINIMAP_PAD: f32 = 10.0;

/// Affine transform mapping diagram world coordinates to minimap-local pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapTransform {
    /// Uniform scale factor from world units to minimap pixels.
    pub scale: f32,
    /// Horizontal pixel offset applied after scaling.
    pub offset_x: f32,
    /// Vertical pixel offset applied after scaling.
    pub offset_y: f32,
}

impl MinimapTransform {
    /// Fits the world rectangle `(min_x..=max_x, min_y..=max_y)` inside `map_w x map_h`
    /// with `pad` pixels of margin on each side, preserving aspect ratio and centering.
    #[must_use]
    pub fn compute(
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        map_w: f32,
        map_h: f32,
        pad: f32,
    ) -> Self {
        let inner_w = (map_w - 2.0 * pad).max(1.0);
        let inner_h = (map_h - 2.0 * pad).max(1.0);
        let world_w = (max_x - min_x).max(1.0);
        let world_h = (max_y - min_y).max(1.0);
        let scale = (inner_w / world_w).min(inner_h / world_h);
        let offset_x = pad + (inner_w - world_w * scale) * 0.5 - min_x * scale;
        let offset_y = pad + (inner_h - world_h * scale) * 0.5 - min_y * scale;
        Self {
            scale,
            offset_x,
            offset_y,
        }
    }

    /// Projects a world coordinate into minimap-local pixels.
    #[must_use]
    pub fn world_to_map(self, wx: f32, wy: f32) -> (f32, f32) {
        (
            wx * self.scale + self.offset_x,
            wy * self.scale + self.offset_y,
        )
    }

    /// Unprojects a minimap-local pixel back into world coordinates.
    #[must_use]
    pub fn map_to_world(self, mx: f32, my: f32) -> (f32, f32) {
        (
            (mx - self.offset_x) / self.scale,
            (my - self.offset_y) / self.scale,
        )
    }

    /// Computes the camera `pan` that centers the viewport (`viewport_w x viewport_h`) at `zoom`
    /// on the world point corresponding to minimap-local `(mx, my)`.
    #[must_use]
    pub fn pan_for_map_point(
        self,
        mx: f32,
        my: f32,
        zoom: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> (f32, f32) {
        let (wx, wy) = self.map_to_world(mx, my);
        (viewport_w * 0.5 - wx * zoom, viewport_h * 0.5 - wy * zoom)
    }
}

#[cfg(test)]
mod tests {
    use super::{MinimapTransform, point_hits_curve, point_hits_polyline};
    use layout::Pos;

    #[test]
    fn hits_horizontal_and_vertical_segments() {
        let pts = [
            Pos { x: 0.0, y: 0.0 },
            Pos { x: 50.0, y: 0.0 },
            Pos { x: 50.0, y: 50.0 },
        ];
        assert!(point_hits_polyline((25.0, 4.0), &pts, 5.0));
        assert!(point_hits_polyline((53.0, 30.0), &pts, 5.0));
        assert!(!point_hits_polyline((25.0, 10.0), &pts, 5.0));
    }

    #[test]
    fn hits_curve_and_misses_point_away_from_it() {
        let curve = diagram_export::relation_curve(0.0, 0.0, 400.0, 100.0);
        assert!(point_hits_curve((330.0, 50.0), &curve, 1.0));
        assert!(!point_hits_curve((330.0, 70.0), &curve, 1.0));
    }

    #[test]
    fn handles_degenerate_zero_length_segment() {
        let pts = [Pos { x: 10.0, y: 10.0 }, Pos { x: 10.0, y: 10.0 }];
        assert!(point_hits_polyline((12.0, 12.0), &pts, 4.0));
        assert!(!point_hits_polyline((20.0, 20.0), &pts, 4.0));
    }

    #[test]
    fn minimap_transform_roundtrips_world_and_centers_camera_pan() {
        let xf = MinimapTransform::compute(-200.0, 100.0, 1000.0, 700.0, 210.0, 136.0, 10.0);
        let (mx, my) = xf.world_to_map(400.0, 400.0);
        // World center (400, 400) maps to minimap center (105, 68)
        assert!((mx - 105.0).abs() < 1e-3, "{mx}");
        assert!((my - 68.0).abs() < 1e-3, "{my}");

        let (wx, wy) = xf.map_to_world(mx, my);
        assert!((wx - 400.0).abs() < 1e-3);
        assert!((wy - 400.0).abs() < 1e-3);

        let (pan_x, pan_y) = xf.pan_for_map_point(mx, my, 1.5, 1200.0, 800.0);
        assert!((pan_x - (600.0 - 400.0 * 1.5)).abs() < 1e-3);
        assert!((pan_y - (400.0 - 400.0 * 1.5)).abs() < 1e-3);
    }
}
