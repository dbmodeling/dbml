use layout::{CARD_W, GROUP_PAD, HEADER_H, ROW_H, TABLE_COLORS, card_h};
use schema::Schema;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Colours used by [`to_svg`], following the canvas palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Opaque canvas background.
    pub background: String,
    /// Card fill.
    pub card: String,
    /// Card border.
    pub edge: String,
    /// Soft divider inside cards.
    pub edge_soft: String,
    /// Primary text.
    pub text: String,
    /// Secondary text.
    pub text_muted: String,
    /// Tertiary text, including column types.
    pub text_faint: String,
    /// PK or selection highlight.
    pub accent: String,
    /// FK glyph.
    pub link: String,
    /// Relation stroke.
    pub relation: String,
    /// Dotted canvas grid.
    pub grid: String,
    /// Dark-mode group and shadow treatment.
    pub dark: bool,
}

impl Theme {
    /// Dark palette.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            background: "#0a0b0c".into(),
            card: "#17181b".into(),
            edge: "rgba(255,255,255,0.122)".into(),
            edge_soft: "rgba(255,255,255,0.071)".into(),
            text: "#ececee".into(),
            text_muted: "#a3a5ab".into(),
            text_faint: "#858890".into(),
            accent: "#e56731".into(),
            link: "#589aed".into(),
            relation: "rgba(220,225,235,0.322)".into(),
            grid: "rgba(255,255,255,0.086)".into(),
            dark: true,
        }
    }

    /// Light palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            background: "#fbfbfa".into(),
            card: "#ffffff".into(),
            edge: "rgba(20,20,30,0.122)".into(),
            edge_soft: "rgba(20,20,30,0.078)".into(),
            text: "#17181b".into(),
            text_muted: "#55585f".into(),
            text_faint: "#6b6e75".into(),
            accent: "#e56731".into(),
            link: "#589aed".into(),
            relation: "rgba(30,30,40,0.302)".into(),
            grid: "rgba(0,0,0,0.122)".into(),
            dark: false,
        }
    }
}

type RelationSelection<'a> = ((&'a str, &'a str), (&'a str, &'a str));

/// Visual selection of the canvas at the time of export.
#[derive(Default)]
pub struct Selection<'a> {
    /// Qualified table names highlighted in the canvas.
    pub tables: &'a [String],
    /// Name of the selected enum.
    pub enum_name: Option<&'a str>,
    /// Selected relationship as two (qualified table name, column name) endpoints.
    pub relation: Option<RelationSelection<'a>>,
}

type GroupBox = (String, String, (f32, f32, f32, f32), usize);

const ENUM_W: f32 = 220.0;
const ENUM_ROW_H: f32 = 26.0;
const PAD: f32 = 48.0;
const FONT: &str = "Geist, sans-serif";
const MONO: &str = "Geist Mono, monospace";

// The canvas uses these same Lucide glyphs through gpui-kit-assets.
const TABLE: &str =
    "<path d=\"M3 9h18M9 3v18\"/><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"2\"/>";
const KEY: &str = "<path d=\"M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z\"/><circle cx=\"16.5\" cy=\"7.5\" r=\".5\"/>";
const LINK: &str = "<path d=\"M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71\"/>";
const LAYERS: &str = "<path d=\"M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83zM2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17\"/>";
const LIST: &str = "<path d=\"M3 5h.01M3 12h.01M3 19h.01M8 5h13M8 12h13M8 19h13\"/>";
const DOT: &str = "<circle cx=\"12\" cy=\"12\" r=\"1\"/>";

fn icon(out: &mut String, shape: &str, color: &str, x: f32, y: f32, size: f32) {
    let _ = writeln!(
        out,
        "    <g transform=\"translate({x} {y}) scale({})\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{shape}</g>",
        size / 24.0
    );
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(s: &str) -> Option<String> {
    let digits = s.trim().trim_start_matches('#');
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{digits}"))
}

fn enum_h(rows: usize) -> f32 {
    HEADER_H + (rows.clamp(1, 6) + usize::from(rows > 6)) as f32 * ENUM_ROW_H + 12.0
}

/// Renders the positioned diagram as a standalone SVG.
#[must_use]
pub fn to_svg<S: std::hash::BuildHasher>(
    schema: &Schema,
    positions: &HashMap<String, (f32, f32), S>,
    edges: &crate::EdgeStyles,
    theme: &Theme,
) -> String {
    to_svg_with_selection(schema, positions, edges, theme, &Selection::default())
}

/// Renders the positioned diagram with its active table, enum and relation highlights.
///
/// Position keys are qualified table names or `enum:<qualified name>`. Unpositioned cards
/// are skipped. `edges` preserves relation curves and straight lines from the layout sidecar.
#[must_use]
pub fn to_svg_with_selection<S: std::hash::BuildHasher>(
    schema: &Schema,
    positions: &HashMap<String, (f32, f32), S>,
    edges: &crate::EdgeStyles,
    theme: &Theme,
    selection: &Selection<'_>,
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
        for t in g.tables.iter().filter_map(|name| schema.table(name)) {
            if let Some((x, y)) = table_pos(t) {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + CARD_W);
                y1 = y1.max(y + card_h(t.columns.len()));
            }
        }
        if n > 0 {
            let color = g
                .color
                .as_deref()
                .and_then(hex)
                .unwrap_or_else(|| TABLE_COLORS[i % TABLE_COLORS.len()].to_owned());
            let pad = GROUP_PAD - 8.0;
            let r = (
                x0 - pad,
                y0 - pad - 8.0,
                x1 - x0 + 2.0 * pad,
                y1 - y0 + 2.0 * pad + 8.0,
            );
            grow(r.0, r.1, r.2, r.3);
            groups.push((g.name.clone(), color, r, n));
        }
    }
    let mut o = String::new();
    // Relações entre as portas das linhas de coluna.
    for r in &schema.refs {
        let (ft, tt) = (r.from.qualified_table(), r.to.qualified_table());
        let (Some(a), Some(b)) = (schema.table(&ft), schema.table(&tt)) else {
            continue;
        };
        let (Some((ax, ay)), Some((bx, by))) = (table_pos(a), table_pos(b)) else {
            continue;
        };
        let row = |table: &schema::Table, col: &str| {
            let i = table.columns.iter().position(|c| c.name == col)?;
            Some(HEADER_H + 4.0 + (i as f32 + 0.5) * ROW_H)
        };
        let (Some(from_y), Some(to_y)) = (row(a, &r.from.column), row(b, &r.to.column)) else {
            continue;
        };
        let (fy, ty) = (ay + from_y, by + to_y);
        let style = edges.get(&crate::edge_key(
            &a.qualified_name(),
            &r.from,
            &b.qualified_name(),
            &r.to,
        ));
        let auto = crate::relation_curve(ax, fy, bx, ty);
        let [start, _, _, end] = auto;
        let segments = if style.is_some() {
            vec![crate::styled_curve(auto, style)]
        } else {
            let obstacles: Vec<layout::Rect> = schema
                .tables
                .iter()
                .filter(|t| !std::ptr::eq(*t, a) && !std::ptr::eq(*t, b))
                .filter_map(|t| {
                    let (x, y) = table_pos(t)?;
                    Some(layout::Rect {
                        x,
                        y,
                        w: CARD_W,
                        h: card_h(t.columns.len()),
                    })
                })
                .collect();
            crate::relation_segments(ax, fy, bx, ty, &obstacles)
        };
        for point in segments.iter().flatten() {
            grow(point.x, point.y, 0.0, 0.0);
        }
        let path = crate::segments_svg_path(&segments);
        let (sx, ex) = (start.x, end.x);
        let (fy, ty) = (start.y, end.y);
        let (from_label, to_label) = match r.kind {
            schema::RefKind::ManyToOne => ("N", "1"),
            schema::RefKind::OneToMany => ("1", "N"),
            schema::RefKind::OneToOne => ("1", "1"),
            schema::RefKind::ManyToMany => ("N", "N"),
        };
        let lx = sx + (auto[1].x - sx).signum() * 11.0;
        let rx = ex + (auto[2].x - ex).signum() * 11.0;
        let active = selection.tables.contains(&ft)
            || selection.tables.contains(&tt)
            || selection.relation
                == Some((
                    (ft.as_str(), r.from.column.as_str()),
                    (tt.as_str(), r.to.column.as_str()),
                ));
        let (stroke, width, label) = if active {
            let _ = writeln!(
                o,
                "  <path d=\"{path}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"0.16\" stroke-width=\"8\"/>",
                theme.accent
            );
            (&theme.accent, 1.8, &theme.accent)
        } else {
            (&theme.relation, 1.4, &theme.text_faint)
        };
        let _ = writeln!(
            o,
            "  <path d=\"{path}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{width}\"/><circle cx=\"{sx}\" cy=\"{fy}\" r=\"3\" fill=\"{stroke}\"/><circle cx=\"{ex}\" cy=\"{ty}\" r=\"3.5\" fill=\"{}\" stroke=\"{stroke}\" stroke-width=\"1.5\"/><text x=\"{lx}\" y=\"{}\" text-anchor=\"middle\" font-family=\"{MONO}\" font-size=\"10\" font-weight=\"500\" fill=\"{label}\">{from_label}</text><text x=\"{rx}\" y=\"{}\" text-anchor=\"middle\" font-family=\"{MONO}\" font-size=\"10\" font-weight=\"500\" fill=\"{label}\">{to_label}</text>",
            theme.card,
            fy - 6.0,
            ty - 6.0,
        );
    }

    let (fill, border) = if theme.dark {
        (0.06, 0.18)
    } else {
        (0.07, 0.22)
    };
    for (name, color, (x, y, gw, gh), n) in &groups {
        let name_x = x + 35.0;
        let pill_w = 12.0 + n.to_string().len() as f32 * 6.3;
        let pill_x = name_x + name.chars().count() as f32 * 6.5 + 6.0;
        let _ = writeln!(
            o,
            "  <rect x=\"{x}\" y=\"{y}\" width=\"{gw}\" height=\"{gh}\" rx=\"20\" fill=\"{color}\" fill-opacity=\"{fill}\" stroke=\"{color}\" stroke-opacity=\"{border}\"/>"
        );
        icon(&mut o, LAYERS, color, x + 16.0, y + 15.5, 13.0);
        let _ = writeln!(
            o,
            "  <text x=\"{name_x}\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"{color}\">{}</text><rect x=\"{pill_x}\" y=\"{}\" width=\"{pill_w}\" height=\"16\" rx=\"8\" fill=\"{color}\" fill-opacity=\"0.14\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"{MONO}\" font-size=\"10.5\" font-weight=\"500\" fill=\"{color}\">{n}</text>",
            y + 26.0,
            esc(name),
            y + 14.0,
            pill_x + pill_w / 2.0,
            y + 26.0
        );
    }

    let fks: HashSet<_> = schema
        .refs
        .iter()
        .map(|r| (r.from.qualified_table(), r.from.column.as_str()))
        .collect();
    let mut tables: Vec<_> = schema.tables.iter().collect();
    tables.sort_by_key(|t| selection.tables.contains(&t.qualified_name()));
    for t in tables {
        let Some((x, y)) = table_pos(t) else { continue };
        let name = t.qualified_name();
        let tint = t.color.as_deref().and_then(hex).or_else(|| {
            let group = schema.group_of(&name)?;
            let index = schema.groups.iter().position(|g| g.name == group.name)?;
            Some(
                group
                    .color
                    .as_deref()
                    .and_then(hex)
                    .unwrap_or_else(|| TABLE_COLORS[index % TABLE_COLORS.len()].to_owned()),
            )
        });
        let h = card_h(t.columns.len());
        let active = selection.tables.contains(&name);
        card(&mut o, theme, (x, y), CARD_W, h, active, false);
        icon(
            &mut o,
            TABLE,
            tint.as_deref().unwrap_or(&theme.accent),
            12.0,
            15.0,
            14.0,
        );
        let _ = writeln!(
            o,
            "    <text x=\"35\" y=\"26\" font-size=\"13\" font-weight=\"600\" fill=\"{}\">{}</text>\n    <text x=\"{}\" y=\"26\" text-anchor=\"end\" font-family=\"{MONO}\" font-size=\"10.5\" font-weight=\"500\" fill=\"{}\">{}</text>",
            theme.text,
            esc(&t.name),
            CARD_W - 16.0,
            theme.text_faint,
            t.columns.len()
        );
        if let Some(schema) = &t.schema {
            let w = 10.0 + schema.chars().count() as f32 * 5.8;
            let x = CARD_W - 43.0 - w;
            let _ = writeln!(
                o,
                "    <rect x=\"{x}\" y=\"15\" width=\"{w}\" height=\"14\" rx=\"4\" fill=\"{}\" fill-opacity=\"0.16\"/><text x=\"{}\" y=\"25.5\" font-size=\"10\" fill=\"{}\">{}</text>",
                theme.accent,
                x + 5.0,
                theme.text_muted,
                esc(schema)
            );
        }
        for (i, c) in t.columns.iter().enumerate() {
            let middle = HEADER_H + 4.0 + (i as f32 + 0.5) * ROW_H;
            let baseline = middle + 4.0;
            let (shape, color) = if c.pk {
                (KEY, &theme.accent)
            } else if c.fk || fks.contains(&(name.clone(), c.name.as_str())) {
                (LINK, &theme.link)
            } else {
                (DOT, &theme.text_faint)
            };
            icon(&mut o, shape, color, 13.0, middle - 6.0, 12.0);
            let right = CARD_W - 12.0;
            let _ = writeln!(
                o,
                "    <text x=\"{right}\" y=\"{baseline}\" text-anchor=\"end\" font-family=\"{MONO}\" font-size=\"11\" fill=\"{}\">{}</text>",
                theme.text_faint,
                esc(&c.ty)
            );
            let mut badge_right = right - c.ty.chars().count() as f32 * 6.6 - 8.0;
            let has_idx = t.indexes.iter().any(|ix| ix.references_column(&c.name));
            for (show, label, ink) in [
                (c.check.is_some(), "CHK", theme.text_faint.as_str()),
                (c.default_value.is_some(), "DEF", theme.text_faint.as_str()),
                (c.unique, "UQ", theme.text_faint.as_str()),
                (c.not_null, "NN", theme.text_faint.as_str()),
                (has_idx, "IDX", TABLE_COLORS[1]),
            ] {
                if show {
                    let _ = writeln!(
                        o,
                        "    <text x=\"{badge_right}\" y=\"{baseline}\" text-anchor=\"end\" font-family=\"{MONO}\" font-size=\"9\" font-weight=\"600\" fill=\"{ink}\">{label}</text>"
                    );
                    badge_right -= label.len() as f32 * 5.4 + 8.0;
                }
            }
            let name_w = (badge_right - 34.0).max(1.0);
            let weight = if c.pk { 500 } else { 400 };
            let _ = writeln!(
                o,
                "    <svg x=\"34\" y=\"{}\" width=\"{name_w}\" height=\"{ROW_H}\" overflow=\"hidden\"><text y=\"19\" font-size=\"12.5\" font-weight=\"{weight}\" fill=\"{}\">{}</text></svg>",
                middle - ROW_H / 2.0,
                theme.text,
                esc(&c.name)
            );
        }
        o.push_str("  </g>\n");
    }

    for e in &schema.enums {
        let qualified = e.qualified_name();
        let Some(&(x, y)) = positions.get(&format!("enum:{qualified}")) else {
            continue;
        };
        let h = enum_h(e.values.len());
        let active = selection
            .enum_name
            .is_some_and(|name| name == e.name || name == qualified);
        card(&mut o, theme, (x, y), ENUM_W, h, active, true);
        icon(&mut o, LIST, &theme.accent, 12.0, 15.0, 14.0);
        let _ = writeln!(
            o,
            "    <text x=\"35\" y=\"26\" font-size=\"13\" font-weight=\"600\" fill=\"{}\">{}</text><text x=\"{}\" y=\"25.5\" text-anchor=\"end\" font-family=\"{MONO}\" font-size=\"9\" font-weight=\"600\" fill=\"{}\">ENUM</text>",
            theme.text,
            esc(&e.name),
            ENUM_W - 12.0,
            theme.text_faint
        );
        for (i, v) in e.values.iter().take(6).enumerate() {
            let middle = HEADER_H + 6.0 + (i as f32 + 0.5) * ENUM_ROW_H;
            icon(&mut o, DOT, &theme.accent, 14.0, middle - 7.0, 14.0);
            let _ = writeln!(
                o,
                "    <text x=\"37\" y=\"{}\" font-family=\"{MONO}\" font-size=\"12.5\" fill=\"{}\">{}</text>",
                middle + 4.0,
                theme.text_muted,
                esc(&v.name)
            );
        }
        let omitted = if e.values.is_empty() {
            Some("(sem valores)".to_owned())
        } else if e.values.len() > 6 {
            Some(format!("+{} a mais", e.values.len() - 6))
        } else {
            None
        };
        if let Some(label) = omitted {
            let y = HEADER_H + 6.0 + (e.values.len().min(6) as f32 + 0.5) * ENUM_ROW_H + 4.0;
            let _ = writeln!(
                o,
                "    <text x=\"14\" y=\"{y}\" font-family=\"{MONO}\" font-size=\"12.5\" fill=\"{}\">{label}</text>",
                theme.text_faint
            );
        }
        o.push_str("  </g>\n");
    }

    if bb.0 > bb.2 {
        bb = (0.0, 0.0, 800.0, 600.0);
    }
    let (vx, vy) = ((bb.0 - PAD).floor(), (bb.1 - PAD).floor());
    let (w, h) = ((bb.2 + PAD - vx).ceil(), (bb.3 + PAD - vy).ceil());
    let shadow = if theme.dark { 0.25 } else { 0.06 };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{vx} {vy} {w} {h}\" width=\"{w}\" height=\"{h}\" font-family=\"{FONT}\">\n  <defs><pattern id=\"canvas-grid\" x=\"-0.75\" y=\"-0.75\" width=\"24\" height=\"24\" patternUnits=\"userSpaceOnUse\"><rect width=\"1.5\" height=\"1.5\" fill=\"{}\"/></pattern><filter id=\"card-shadow\" x=\"-20%\" y=\"-30%\" width=\"140%\" height=\"160%\"><feDropShadow dy=\"2\" stdDeviation=\"4\" flood-opacity=\"{shadow}\"/></filter></defs>\n  <rect x=\"{vx}\" y=\"{vy}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>\n  <rect x=\"{vx}\" y=\"{vy}\" width=\"{w}\" height=\"{h}\" fill=\"url(#canvas-grid)\"/>\n{o}</svg>\n",
        theme.grid, theme.background
    )
}

fn card(
    out: &mut String,
    theme: &Theme,
    (x, y): (f32, f32),
    width: f32,
    height: f32,
    active: bool,
    is_enum: bool,
) {
    let _ = writeln!(out, "  <g transform=\"translate({x} {y})\">");
    let edge = if active {
        let _ = writeln!(
            out,
            "    <rect width=\"{width}\" height=\"{height}\" rx=\"12\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"0.18\" stroke-width=\"6\"/>",
            theme.accent
        );
        &theme.accent
    } else if is_enum {
        &theme.edge
    } else {
        &theme.edge_soft
    };
    let _ = writeln!(
        out,
        "    <rect width=\"{width}\" height=\"{height}\" rx=\"12\" fill=\"{}\" stroke=\"{edge}\" filter=\"url(#card-shadow)\"/><path d=\"M0 {HEADER_H}H{width}\" stroke=\"{}\"/>",
        theme.card, theme.edge_soft
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporta_curva_e_extremidades_da_relacao() {
        let table = |name: &str| schema::Table {
            name: name.into(),
            columns: vec![schema::Column {
                name: "id".into(),
                ty: "int".into(),
                pk: name == "b",
                unique: name == "a",
                not_null: name == "a",
                ..Default::default()
            }],
            ..Default::default()
        };
        let endpoint = |table: &str| schema::Endpoint {
            schema: None,
            table: table.into(),
            column: "id".into(),
            columns: vec![],
        };
        let schema = Schema {
            tables: vec![table("a"), table("b")],
            groups: vec![schema::Group {
                name: "sample".into(),
                tables: vec!["a".into(), "b".into()],
                color: Some("#4ab074".into()),
            }],
            refs: vec![schema::Ref {
                from: endpoint("a"),
                to: endpoint("b"),
                kind: schema::RefKind::ManyToOne,
                on_delete: None,
                on_update: None,
            }],
            ..Default::default()
        };
        let positions = HashMap::from([("a".into(), (0.0, 0.0)), ("b".into(), (400.0, 40.0))]);
        let none = crate::EdgeStyles::new();
        let svg = to_svg(&schema, &positions, &none, &Theme::dark());
        assert!(svg.contains("M 260.00 63.00 C 330.00 63.00 330.00 103.00 400.00 103.00"));
        assert!(svg.contains("cx=\"260\" cy=\"63\" r=\"3\""));
        assert!(svg.contains("cx=\"400\" cy=\"103\" r=\"3.5\""));
        assert!(svg.contains(">N</text>"));
        assert!(svg.contains(">1</text>"));
        assert!(svg.contains("fill=\"url(#canvas-grid)\""));
        assert!(svg.contains("height=\"178\" rx=\"20\" fill=\"#4ab074\""));
        assert!(svg.contains("stroke=\"#4ab074\" stroke-width=\"2\""));
        assert!(svg.contains("stroke=\"#589aed\" stroke-width=\"2\"") && svg.contains(LINK));
        assert!(svg.contains("stroke=\"#e56731\" stroke-width=\"2\"") && svg.contains(KEY));
        assert!(svg.contains("font-family=\"Geist Mono, monospace\""));
        assert!(svg.contains(">NN</text>") && svg.contains(">UQ</text>"));
        assert!(!svg.contains(">PK</text>"));

        let selected = to_svg_with_selection(
            &schema,
            &positions,
            &none,
            &Theme::light(),
            &Selection {
                tables: &["a".into()],
                ..Selection::default()
            },
        );
        assert!(selected.contains("fill=\"#fbfbfa\""));
        assert!(selected.contains("stroke=\"#e56731\" stroke-width=\"1.8\""));
        assert!(selected.contains("stroke-opacity=\"0.18\" stroke-width=\"6\""));
        // o export honra o estilo do sidecar
        let bent = crate::EdgeStyles::from([(
            "a.id->b.id".into(),
            crate::EdgeStyle {
                straight: false,
                dx: 10.0,
                dy: -20.0,
            },
        )]);
        let svg = to_svg(&schema, &positions, &bent, &Theme::dark());
        assert!(
            svg.contains("M 260.00 63.00 C 340.00 43.00 340.00 83.00 400.00 103.00"),
            "{svg}"
        );
        let bent = crate::EdgeStyles::from([(
            "a.id->b.id".into(),
            crate::EdgeStyle {
                dx: 900.0,
                dy: -400.0,
                ..Default::default()
            },
        )]);
        let svg = to_svg(&schema, &positions, &bent, &Theme::dark());
        assert!(svg.contains("viewBox=\"-72 -385 1350 579\""), "{svg}");
    }
}
