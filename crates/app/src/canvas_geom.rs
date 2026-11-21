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

#[cfg(test)]
mod tests {
    use super::{point_hits_curve, point_hits_polyline};
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
}
