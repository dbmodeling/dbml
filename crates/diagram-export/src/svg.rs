use layout::{CARD_W, GROUP_PAD, HEADER_H, ROW_H, TABLE_COLORS, card_h};
use schema::Schema;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Colours used by [`to_svg`]; values are any SVG colour string (`#rrggbb`, `rgba(...)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Page background (opaque, so JPEG export has a solid ground).
    pub background: String,
    /// Card fill.
    pub card: String,
    /// Card border.
    pub edge: String,
    /// Soft divider inside cards.
    pub edge_soft: String,
    /// Primary text.
    pub text: String,
    /// Secondary text, relation lines.
    pub text_muted: String,
    /// Accent (primary-key chip).
    pub accent: String,
}

impl Theme {
    /// Dark palette (matches `crates/app/src/glass.rs`).
    #[must_use]
    pub fn dark() -> Self {
        Self {
            background: "#0a0a0b".into(),
            card: "#151516".into(),
            edge: "rgba(255,255,255,0.10)".into(),
            edge_soft: "rgba(255,255,255,0.05)".into(),
            text: "#ececed".into(),
            text_muted: "#a3a6ad".into(),
            accent: "#c6f240".into(),
        }
    }

    /// Light palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            background: "#f7f7f8".into(),
            card: "#ffffff".into(),
            edge: "rgba(31,35,40,0.13)".into(),
            edge_soft: "rgba(31,35,40,0.08)".into(),
            text: "#1f2328".into(),
            text_muted: "#57606a".into(),
            accent: "#4d7c0f".into(),
        }
    }
}

/// (nome, cor, retângulo x/y/w/h, nº de tabelas)
type GroupBox = (String, String, (f32, f32, f32, f32), usize);

const ENUM_W: f32 = 200.0;
const PAD: f32 = 48.0;
const FONT: &str = "Inter, sans-serif";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn enum_h(rows: usize) -> f32 {
    layout::HEADER_H + rows.min(6) as f32 * 20.0 + 16.0 + if rows > 6 { 20.0 } else { 0.0 }
}

/// Renders the diagram as a standalone SVG.
///
/// `positions` maps a table's qualified name (and `enum:<name>` for enums) to its top-left corner,
/// exactly as the canvas holds them. Cards without a position are skipped.
#[must_use]
pub fn to_svg<S: std::hash::BuildHasher>(
    schema: &Schema,
    positions: &HashMap<String, (f32, f32), S>,
    theme: &Theme,
) -> String {
    // (min_x, min_y, max_x, max_y)
    let mut bb = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut grow = |x: f32, y: f32, w: f32, h: f32| {
        bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x + w), bb.3.max(y + h));
    };
    let table_pos = |t: &schema::Table| positions.get(&t.qualified_name()).copied();
    for t in &schema.tables {
        if let Some((x, y)) = table_pos(t) {
            grow(x, y, CARD_W, card_h(t.columns.len()));
        }
    }
    for e in &schema.enums {
        if let Some(&(x, y)) = positions.get(&format!("enum:{}", e.qualified_name())) {
            grow(x, y, ENUM_W, enum_h(e.values.len()));
        }
    }
    let mut groups: Vec<GroupBox> = Vec::new();
    for (i, g) in schema.groups.iter().enumerate() {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut n = 0;
        for t in schema
            .tables
            .iter()
            .filter(|t| g.tables.contains(&t.qualified_name()))
        {
            if let Some((x, y)) = table_pos(t) {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + CARD_W);
                y1 = y1.max(y + card_h(t.columns.len()));
            }
        }
        if n > 0 {
            let color = g.color.clone().unwrap_or_else(|| {
                TABLE_COLORS
                    .get(i % TABLE_COLORS.len())
                    .map_or("#3b82f6", |c| c)
                    .to_owned()
            });
            let r = (
                x0 - GROUP_PAD / 2.0,
                y0 - GROUP_PAD,
                x1 - x0 + GROUP_PAD,
                y1 - y0 + GROUP_PAD * 1.5,
            );
            grow(r.0, r.1, r.2, r.3);
            groups.push((g.name.clone(), color, r, n));
        }
    }
    if bb.0 > bb.2 {
        bb = (0.0, 0.0, 800.0, 600.0);
    }
    let (vx, vy) = ((bb.0 - PAD).floor(), (bb.1 - PAD).floor());
    let (w, h) = (
        (bb.2 - bb.0 + PAD * 2.0).ceil(),
        (bb.3 - bb.1 + PAD * 2.0).ceil(),
    );

    let mut o = String::new();
    let _ = writeln!(
        o,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{vx} {vy} {w} {h}\" width=\"{w}\" height=\"{h}\" font-family=\"{FONT}\">"
    );
    let _ = writeln!(
        o,
        "  <rect x=\"{vx}\" y=\"{vy}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>",
        theme.background
    );

    for (name, color, (x, y, gw, gh), n) in &groups {
        let _ = writeln!(
            o,
            "  <rect x=\"{x}\" y=\"{y}\" width=\"{gw}\" height=\"{gh}\" rx=\"14\" fill=\"{color}\" fill-opacity=\"0.06\" stroke=\"{color}\" stroke-opacity=\"0.4\"/>\n  <text x=\"{}\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"{color}\">{} ({n})</text>",
            x + 12.0,
            y + 20.0,
            esc(name)
        );
    }

    // relations: simple orthogonal elbow between the two column rows
    for r in &schema.refs {
        let (ft, tt) = (r.from.qualified_table(), r.to.qualified_table());
        let find = |q: &str| {
            schema
                .tables
                .iter()
                .find(|t| t.qualified_name() == q || (!q.contains('.') && t.name == q))
        };
        let (Some(a), Some(b)) = (find(&ft), find(&tt)) else {
            continue;
        };
        let (Some((ax, ay)), Some((bx, by))) = (table_pos(a), table_pos(b)) else {
            continue;
        };
        let row = |t: &schema::Table, col: &str| {
            HEADER_H
                + 4.0
                + (t.columns.iter().position(|c| c.name == col).unwrap_or(0) as f32 + 0.5) * ROW_H
        };
        let (fy, ty) = (ay + row(a, &r.from.column), by + row(b, &r.to.column));
        let from_left = ax + CARD_W < bx;
        let (sx, ex) = if from_left {
            (ax + CARD_W, bx)
        } else {
            (ax, bx + CARD_W)
        };
        let mid = f32::midpoint(sx, ex);
        let _ = writeln!(
            o,
            "  <path d=\"M {sx} {fy} L {mid} {fy} L {mid} {ty} L {ex} {ty}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"0.55\" stroke-width=\"1.5\"/>",
            theme.text_muted
        );
    }

    for t in &schema.tables {
        let Some((x, y)) = table_pos(t) else { continue };
        let h = card_h(t.columns.len());
        let tint = t.color.clone().or_else(|| {
            schema
                .groups
                .iter()
                .position(|g| g.tables.contains(&t.qualified_name()))
                .and_then(|i| TABLE_COLORS.get(i % TABLE_COLORS.len()))
                .map(|c| (*c).to_owned())
        });
        let _ = writeln!(
            o,
            "  <g transform=\"translate({x} {y})\">\n    <rect width=\"{CARD_W}\" height=\"{h}\" rx=\"12\" fill=\"{}\" stroke=\"{}\"/>",
            theme.card, theme.edge
        );
        if let Some(c) = tint {
            let _ = writeln!(
                o,
                "    <rect width=\"{CARD_W}\" height=\"{HEADER_H}\" rx=\"12\" fill=\"{c}\" fill-opacity=\"0.08\"/>\n    <rect width=\"{CARD_W}\" height=\"3\" rx=\"2\" fill=\"{c}\"/>"
            );
        }
        let _ = writeln!(
            o,
            "    <line x1=\"0\" y1=\"{HEADER_H}\" x2=\"{CARD_W}\" y2=\"{HEADER_H}\" stroke=\"{}\"/>\n    <text x=\"16\" y=\"27\" font-size=\"14\" font-weight=\"600\" fill=\"{}\">{}</text>\n    <text x=\"{}\" y=\"26\" text-anchor=\"end\" font-size=\"11.5\" fill=\"{}\">{}</text>",
            theme.edge_soft,
            theme.text,
            esc(&t.qualified_name()),
            CARD_W - 16.0,
            theme.text_muted,
            t.columns.len()
        );
        for (i, c) in t.columns.iter().enumerate() {
            let cy = HEADER_H + 4.0 + i as f32 * ROW_H + 19.0;
            if c.pk {
                let _ = writeln!(
                    o,
                    "    <text x=\"12\" y=\"{cy}\" font-size=\"10\" font-weight=\"600\" fill=\"{}\">PK</text>",
                    theme.accent
                );
            }
            let tx = if c.not_null {
                CARD_W - 34.0
            } else {
                CARD_W - 12.0
            };
            let _ = writeln!(
                o,
                "    <text x=\"34\" y=\"{cy}\" font-size=\"13\" fill=\"{}\">{}</text>\n    <text x=\"{tx}\" y=\"{cy}\" text-anchor=\"end\" font-size=\"11.5\" fill=\"{}\">{}</text>",
                theme.text,
                esc(&c.name),
                theme.text_muted,
                esc(&c.ty)
            );
            if c.not_null {
                let _ = writeln!(
                    o,
                    "    <text x=\"{}\" y=\"{cy}\" text-anchor=\"end\" font-size=\"10\" fill=\"{}\">NN</text>",
                    CARD_W - 12.0,
                    theme.text_muted
                );
            }
        }
        o.push_str("  </g>\n");
    }

    for e in &schema.enums {
        let Some(&(x, y)) = positions.get(&format!("enum:{}", e.qualified_name())) else {
            continue;
        };
        let h = enum_h(e.values.len());
        let _ = writeln!(
            o,
            "  <g transform=\"translate({x} {y})\">\n    <rect width=\"{ENUM_W}\" height=\"{h}\" rx=\"12\" fill=\"{}\" stroke=\"{}\"/>\n    <text x=\"16\" y=\"27\" font-size=\"14\" font-weight=\"600\" fill=\"{}\">{}</text>\n    <text x=\"{}\" y=\"26\" text-anchor=\"end\" font-size=\"10\" fill=\"{}\">ENUM</text>",
            theme.card,
            theme.edge,
            theme.text,
            esc(&e.qualified_name()),
            ENUM_W - 16.0,
            theme.accent
        );
        for (i, v) in e.values.iter().take(6).enumerate() {
            let _ = writeln!(
                o,
                "    <text x=\"16\" y=\"{}\" font-size=\"12\" fill=\"{}\">{}</text>",
                HEADER_H + 14.0 + i as f32 * 20.0,
                theme.text_muted,
                esc(&v.name)
            );
        }
        if e.values.len() > 6 {
            let _ = writeln!(
                o,
                "    <text x=\"16\" y=\"{}\" font-size=\"11\" fill=\"{}\">+{} more</text>",
                HEADER_H + 14.0 + 6.0 * 20.0,
                theme.text_muted,
                e.values.len() - 6
            );
        }
        o.push_str("  </g>\n");
    }
    o.push_str("</svg>\n");
    o
}
