//! Deterministic pipeline benchmarks for parse, analyze, lint, layout and route (#116).
//!
//! Run with: `cargo run --release -p dbml-wasm --example bench_pipeline`

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

const TABLE_COUNTS: [usize; 3] = [50, 200, 500];
const WARMUP_ITERS: usize = 2;
const SAMPLE_ITERS: usize = 5;
const GROUP_SIZE: usize = 10;
const EXTRA_COLS: usize = 4;
const ROUTE_GAP_PX: f32 = 16.0;
const ROUTE_OFFSET_Y: f32 = 48.0;

fn generate_schema(n_tables: usize) -> String {
    let mut out = String::with_capacity(n_tables * 180);
    out.push_str("Enum status {\n  draft\n  active\n  archived\n}\n\n");
    for i in 0..n_tables {
        let _ = writeln!(out, "Table t_{i} {{");
        out.push_str("  id int [pk]\n");
        if i > 0 {
            let _ = writeln!(out, "  parent_id int [ref: > t_{}.id]", i - 1);
        }
        for c in 0..EXTRA_COLS {
            let _ = writeln!(out, "  col_{c} varchar");
        }
        out.push_str("  state status\n}\n\n");
    }
    for g in 0..(n_tables / GROUP_SIZE) {
        let _ = writeln!(out, "TableGroup grp_{g} {{");
        for i in (g * GROUP_SIZE)..((g + 1) * GROUP_SIZE) {
            let _ = writeln!(out, "  t_{i}");
        }
        out.push_str("}\n\n");
    }
    out
}

fn measure_ms<F: FnMut()>(mut f: F) -> f64 {
    for _ in 0..WARMUP_ITERS {
        f();
    }
    let start = Instant::now();
    for _ in 0..SAMPLE_ITERS {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / SAMPLE_ITERS as f64
}

fn main() {
    println!(
        "{:<8} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "tables", "parse_ms", "analyze_ms", "lint_ms", "layout_ms", "route_ms"
    );
    for n in TABLE_COUNTS {
        let src = generate_schema(n);
        let parse_ms = measure_ms(|| {
            black_box(dbml_syntax::parse(&src));
        });
        let analyze_ms = measure_ms(|| {
            black_box(dbml_analysis::analyze(&src));
        });
        let lint_ms = measure_ms(|| {
            black_box(dbml_analysis::lint(&src));
        });
        let schema = dbml_analysis::analyze(&src);
        let col_counts: Vec<usize> = schema.tables.iter().map(|t| t.columns.len()).collect();
        let idx_by_name: std::collections::HashMap<String, usize> = schema
            .tables
            .iter()
            .enumerate()
            .map(|(i, t)| (t.qualified_name(), i))
            .collect();
        let edges: Vec<(usize, usize)> = schema
            .refs
            .iter()
            .filter_map(|r| {
                Some((
                    *idx_by_name.get(&r.from.qualified_table())?,
                    *idx_by_name.get(&r.to.qualified_table())?,
                ))
            })
            .collect();
        let groups: Vec<Vec<usize>> = schema
            .groups
            .iter()
            .map(|g| {
                g.tables
                    .iter()
                    .filter_map(|t| idx_by_name.get(t).copied())
                    .collect()
            })
            .collect();
        let layout_ms = measure_ms(|| {
            black_box(layout::compute_diagram_positions(
                &col_counts,
                &edges,
                Some(&groups),
            ));
        });
        let positions = layout::compute_diagram_positions(&col_counts, &edges, Some(&groups));
        let obstacles: Vec<layout::Rect> = schema
            .tables
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let p = positions.get(i)?;
                Some(layout::Rect {
                    x: p.x,
                    y: p.y,
                    w: layout::CARD_W,
                    h: layout::card_h(t.columns.len()),
                })
            })
            .collect();
        let route_ms = measure_ms(|| {
            for &(from, to) in &edges {
                let (Some(a), Some(b)) = (positions.get(from), positions.get(to)) else {
                    continue;
                };
                black_box(layout::route(
                    layout::Pos {
                        x: a.x + layout::CARD_W,
                        y: a.y + ROUTE_OFFSET_Y,
                    },
                    1.0,
                    layout::Pos {
                        x: b.x,
                        y: b.y + ROUTE_OFFSET_Y,
                    },
                    -1.0,
                    &obstacles,
                    ROUTE_GAP_PX,
                ));
            }
        });
        println!(
            "{n:<8} {parse_ms:>10.2} {analyze_ms:>10.2} {lint_ms:>10.2} {layout_ms:>10.2} {route_ms:>10.2}"
        );
    }
}
