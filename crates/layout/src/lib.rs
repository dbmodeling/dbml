#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Deterministic layered auto-layout (simplified Sugiyama) and orthogonal edge routing.

/// Default table card width in world pixels.
pub const CARD_W: f32 = 260.0;
/// Table card header height in world pixels.
pub const HEADER_H: f32 = 44.0;
/// Single column row height in world pixels.
pub const ROW_H: f32 = 30.0;
/// Inner padding around grouped table cards in world pixels.
pub const GROUP_PAD: f32 = 32.0;

/// Canonical 8-color palette for tables and groups (matches `crates/app/src/glass.rs`).
pub const TABLE_COLORS: &[&str] = &[
    "#3b82f6", // Blue
    "#06b6d4", // Cyan
    "#22c55e", // Green
    "#84cc16", // Lime
    "#f59e0b", // Amber
    "#f97316", // Orange
    "#ec4899", // Pink
    "#8b5cf6", // Violet
];

/// Computes the height of a table card with `cols` columns in world pixels.
#[must_use]
pub fn card_h(cols: usize) -> f32 {
    HEADER_H + ROW_H * cols as f32 + 8.0
}

/// Computes diagram positions for `table_col_counts` using canonical card dimensions,
/// `edges`, and optional `clusters` (`TableGroup` indices + ungrouped tail cluster).
#[must_use]
pub fn compute_diagram_positions(
    table_col_counts: &[usize],
    edges: &[(usize, usize)],
    clusters: Option<&[Vec<usize>]>,
) -> Vec<Pos> {
    let sizes: Vec<_> = table_col_counts
        .iter()
        .map(|&cols| Size {
            w: CARD_W,
            h: card_h(cols),
        })
        .collect();
    let sp = Spacing {
        gap_x: 104.0,
        gap_y: 40.0,
        margin: 0.0,
    };
    if let Some(clusters) = clusters {
        let row = ((table_col_counts.len() as f32).sqrt() * 760.0).max(1650.0);
        clustered(&sizes, edges, clusters, &sp, GROUP_PAD, row)
    } else {
        layered(&sizes, edges, &sp)
    }
}

/// 2D bounding box dimensions in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    /// Width in logical pixels.
    pub w: f32,
    /// Height in logical pixels.
    pub h: f32,
}

/// 2D position in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pos {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

/// Spacing parameters for layered layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Horizontal gap between adjacent layers.
    pub gap_x: f32,
    /// Vertical gap between nodes in the same layer.
    pub gap_y: f32,
    /// Outer margin around the layout.
    pub margin: f32,
}

/// Computes layered positions for nodes of `sizes` connected by `edges`.
///
/// Each edge `(source, target)` places `target` (the referenced table) in a layer
/// to the left of `source`. Returns one [`Pos`] per node in `sizes`.
#[must_use]
pub fn layered(sizes: &[Size], edges: &[(usize, usize)], sp: &Spacing) -> Vec<Pos> {
    let n = sizes.len();
    let layer = layers(n, edges);
    let depth = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut cols: Vec<Vec<usize>> = vec![Vec::new(); depth];
    for (i, &l) in layer.iter().enumerate() {
        cols[l].push(i);
    }

    // baricentro: 2 varreduras (esq→dir pelos vizinhos à esquerda, depois dir→esq)
    let mut order = vec![0f32; n];
    for col in &cols {
        for (r, &i) in col.iter().enumerate() {
            order[i] = r as f32;
        }
    }
    for pass in 0..4 {
        let range: Vec<usize> = if pass % 2 == 0 {
            (1..depth).collect()
        } else {
            (0..depth.saturating_sub(1)).rev().collect()
        };
        for c in range {
            let mut keyed: Vec<(f32, usize)> = cols[c]
                .iter()
                .map(|&i| {
                    let (sum, count) = edges
                        .iter()
                        .filter_map(|&(a, b)| match (a == i, b == i) {
                            (true, _) => Some(b),
                            (_, true) => Some(a),
                            _ => None,
                        })
                        .filter(|&o| {
                            (pass % 2 == 0 && layer[o] + 1 == c)
                                || (pass % 2 == 1 && layer[o] == c + 1)
                        })
                        .map(|o| order[o])
                        .fold((0.0_f32, 0_usize), |(acc, cnt), v| (acc + v, cnt + 1));
                    let bary = if count == 0 {
                        order[i]
                    } else {
                        sum / count as f32
                    };
                    (bary, i)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            cols[c] = keyed.iter().map(|k| k.1).collect();
            for (r, &i) in cols[c].iter().enumerate() {
                order[i] = r as f32;
            }
        }
    }

    let mut out = vec![Pos { x: 0., y: 0. }; n];
    let mut x = sp.margin;
    for col in &cols {
        let mut y = sp.margin;
        let mut w: f32 = 0.;
        for &i in col {
            out[i] = Pos { x, y };
            y += sizes[i].h + sp.gap_y;
            w = w.max(sizes[i].w);
        }
        x += w + sp.gap_x;
    }
    out
}

/// Camada de cada nó = maior distância até uma tabela sem dependências (arestas
/// de retorno de ciclos são ignoradas via DFS).
fn layers(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    fn visit(i: usize, deps: &[Vec<usize>], state: &mut [u8], layer: &mut [usize]) {
        state[i] = 1;
        let mut l = 0;
        for &d in &deps[i] {
            match state[d] {
                0 => {
                    visit(d, deps, state, layer);
                    l = l.max(layer[d] + 1);
                }
                2 => l = l.max(layer[d] + 1),
                _ => {} // aresta de retorno (ciclo): ignorada
            }
        }
        layer[i] = l;
        state[i] = 2;
    }
    // dependências: a -> b significa "a referencia b"; b fica antes
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b && a < n && b < n && !deps[a].contains(&b) {
            deps[a].push(b);
        }
    }
    let mut state = vec![0u8; n]; // 0 novo, 1 na pilha, 2 pronto
    let mut layer = vec![0usize; n];
    for i in 0..n {
        if state[i] == 0 {
            visit(i, &deps, &mut state, &mut layer);
        }
    }
    layer
}

/// Computes clustered layered layout for grouped tables, wrapping groups into rows
/// up to `row_width` with internal group padding `pad`.
#[must_use]
pub fn clustered(
    sizes: &[Size],
    edges: &[(usize, usize)],
    clusters: &[Vec<usize>],
    sp: &Spacing,
    pad: f32,
    row_width: f32,
) -> Vec<Pos> {
    let mut out = vec![Pos { x: 0., y: 0. }; sizes.len()];
    let (mut x, mut y, mut row_h) = (sp.margin, sp.margin, 0f32);
    for members in clusters.iter().filter(|m| !m.is_empty()) {
        let local: std::collections::HashMap<usize, usize> =
            members.iter().enumerate().map(|(i, &g)| (g, i)).collect();
        let sub_sizes: Vec<Size> = members.iter().map(|&i| sizes[i]).collect();
        let sub_edges: Vec<(usize, usize)> = edges
            .iter()
            .filter_map(|(a, b)| Some((*local.get(a)?, *local.get(b)?)))
            .collect();
        let inner = Spacing { margin: 0., ..*sp };
        let pos = layered(&sub_sizes, &sub_edges, &inner);
        let w = pos
            .iter()
            .zip(&sub_sizes)
            .map(|(p, s)| p.x + s.w)
            .fold(0., f32::max)
            + 2. * pad;
        let h = pos
            .iter()
            .zip(&sub_sizes)
            .map(|(p, s)| p.y + s.h)
            .fold(0., f32::max)
            + 2. * pad;
        if x > sp.margin && x + w > row_width {
            x = sp.margin;
            y += row_h + sp.gap_y * 2.;
            row_h = 0.;
        }
        for (k, &g) in members.iter().enumerate() {
            out[g] = Pos {
                x: x + pad + pos[k].x,
                y: y + pad + pos[k].y,
            };
        }
        x += w + sp.gap_x;
        row_h = row_h.max(h);
    }
    out
}

/// Axis-aligned rectangle used as an obstacle during orthogonal edge routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge X coordinate.
    pub x: f32,
    /// Top edge Y coordinate.
    pub y: f32,
    /// Rectangle width.
    pub w: f32,
    /// Rectangle height.
    pub h: f32,
}

impl Rect {
    fn hit_h(&self, y: f32, x0: f32, x1: f32) -> bool {
        let (lo, hi) = (x0.min(x1), x0.max(x1));
        y > self.y && y < self.y + self.h && hi > self.x && lo < self.x + self.w
    }
    fn hit_v(&self, x: f32, y0: f32, y1: f32) -> bool {
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        x > self.x && x < self.x + self.w && hi > self.y && lo < self.y + self.h
    }
}

fn blocked(path: &[Pos], obstacles: &[Rect]) -> bool {
    path.windows(2).any(|s| {
        let (p, q) = (s[0], s[1]);
        obstacles.iter().any(|r| {
            if (p.y - q.y).abs() < 0.01 {
                r.hit_h(p.y, p.x, q.x)
            } else {
                r.hit_v(p.x, p.y, q.y)
            }
        })
    })
}

fn length(path: &[Pos]) -> f32 {
    path.windows(2)
        .map(|s| (s[0].x - s[1].x).abs() + (s[0].y - s[1].y).abs())
        .sum()
}

/// Computes an orthogonal route from `a` (exiting side `sa`: `+1.0` right, `-1.0` left)
/// to `b` (entering side `sb`), avoiding `obstacles`.
#[must_use]
pub fn route(a: Pos, sa: f32, b: Pos, sb: f32, obstacles: &[Rect], gap: f32) -> Vec<Pos> {
    let facing = (sa > 0. && sb < 0. && a.x < b.x) || (sa < 0. && sb > 0. && a.x > b.x);
    let mx = if facing {
        f32::midpoint(a.x, b.x)
    } else if sa > 0. {
        a.x.max(b.x) + gap
    } else {
        a.x.min(b.x) - gap
    };
    let simple = vec![a, Pos { x: mx, y: a.y }, Pos { x: mx, y: b.y }, b];
    if !blocked(&simple, obstacles) {
        return simple;
    }
    let (x1, x2) = (a.x + sa * gap, b.x + sb * gap);
    let (lo, hi) = (x1.min(x2), x1.max(x2));
    let mut ys = vec![a.y, b.y];
    for r in obstacles.iter().filter(|r| r.x < hi && r.x + r.w > lo) {
        ys.push(r.y - gap);
        ys.push(r.y + r.h + gap);
    }
    ys.into_iter()
        .map(|cy| {
            vec![
                a,
                Pos { x: x1, y: a.y },
                Pos { x: x1, y: cy },
                Pos { x: x2, y: cy },
                Pos { x: x2, y: b.y },
                b,
            ]
        })
        .filter(|p| !blocked(p, obstacles))
        .min_by(|p, q| length(p).total_cmp(&length(q)))
        .unwrap_or(simple)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SP: Spacing = Spacing {
        gap_x: 100.,
        gap_y: 20.,
        margin: 10.,
    };
    fn sz(n: usize) -> Vec<Size> {
        vec![Size { w: 50., h: 40. }; n]
    }

    #[test]
    fn referenced_table_is_left_of_referencing() {
        // 1 referencia 0
        let p = layered(&sz(2), &[(1, 0)], &SP);
        assert!(p[0].x < p[1].x);
    }

    #[test]
    fn chain_and_no_overlap() {
        let edges = [(1, 0), (2, 1), (3, 1)];
        let p = layered(&sz(4), &edges, &SP);
        assert!(p[0].x < p[1].x && p[1].x < p[2].x && (p[2].x - p[3].x).abs() < f32::EPSILON);
        assert!((p[2].y - p[3].y).abs() >= 40.);
    }

    #[test]
    fn cycle_terminates() {
        let p = layered(&sz(3), &[(0, 1), (1, 2), (2, 0)], &SP);
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn route_direct_when_clear() {
        let p = route(
            Pos { x: 0., y: 0. },
            1.,
            Pos { x: 100., y: 50. },
            -1.,
            &[],
            10.,
        );
        assert_eq!(p.len(), 4);
        assert_eq!(p[1].x, 50.);
    }

    #[test]
    fn route_goes_around_card_in_the_way() {
        // cartão entre a e b, cobrindo a linha reta
        let wall = Rect {
            x: 40.,
            y: -100.,
            w: 20.,
            h: 300.,
        };
        let p = route(
            Pos { x: 0., y: 0. },
            1.,
            Pos { x: 100., y: 0. },
            -1.,
            &[wall],
            10.,
        );
        assert!(!blocked(&p, &[wall]), "{p:?}");
        assert!(p.iter().any(|q| q.y <= -110. || q.y >= 210.), "{p:?}");
    }

    #[test]
    fn clusters_do_not_overlap() {
        let sizes = sz(4);
        let p = clustered(
            &sizes,
            &[(1, 0), (3, 2)],
            &[vec![0, 1], vec![2, 3]],
            &SP,
            20.,
            10_000.,
        );
        let rect = |i: usize| Rect {
            x: p[i].x,
            y: p[i].y,
            w: 50.,
            h: 40.,
        };
        // bloco 2 começa depois do fim do bloco 1
        let end1 = rect(0).x.max(rect(1).x) + 50.;
        assert!(rect(2).x.min(rect(3).x) > end1, "{p:?}");
        // dentro do bloco, a ordem em camadas se mantém
        assert!(p[0].x < p[1].x && p[2].x < p[3].x);
    }

    #[test]
    fn clusters_wrap_rows() {
        let p = clustered(&sz(2), &[], &[vec![0], vec![1]], &SP, 0., 60.);
        assert!(
            p[1].y > p[0].y,
            "segundo bloco vai para a linha de baixo: {p:?}"
        );
    }

    #[test]
    fn empty_and_isolated() {
        assert_eq!(layered(&[], &[], &SP), [] as [Pos; 0]);
        let p = layered(&sz(3), &[], &SP);
        assert!(p.iter().all(|q| (q.x - 10.).abs() < f32::EPSILON)); // todos na camada 0, empilhados
    }
}
