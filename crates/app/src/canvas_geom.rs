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

#[cfg(test)]
mod tests {
    use super::point_hits_polyline;
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
    fn handles_degenerate_zero_length_segment() {
        let pts = [Pos { x: 10.0, y: 10.0 }, Pos { x: 10.0, y: 10.0 }];
        assert!(point_hits_polyline((12.0, 12.0), &pts, 4.0));
        assert!(!point_hits_polyline((20.0, 20.0), &pts, 4.0));
    }
}
