//! Static navigable documentation generator (Markdown and self-contained HTML) from a [`Schema`].

use crate::to_mermaid;
use schema::{Column, EnumDef, Ref, RefKind, Schema, Table};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn esc_md_cell(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\n', '\r'], "<br>")
}

fn esc_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn ref_cardinality_label(kind: RefKind) -> &'static str {
    match kind {
        RefKind::ManyToOne => "N:1 (`>`)",
        RefKind::OneToMany => "1:N (`<`)",
        RefKind::OneToOne => "1:1 (`-`)",
        RefKind::ManyToMany => "N:N (`<>`)",
    }
}

fn ref_cardinality_badge(kind: RefKind) -> &'static str {
    match kind {
        RefKind::ManyToOne => "N:1",
        RefKind::OneToMany => "1:N",
        RefKind::OneToOne => "1:1",
        RefKind::ManyToMany => "N:N",
    }
}

fn fk_targets_for(schema: &Schema) -> HashMap<(String, String), Vec<(String, String)>> {
    let mut map: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    for r in &schema.refs {
        let from_t = schema
            .table(&r.from.qualified_table())
            .map_or_else(|| r.from.qualified_table(), Table::qualified_name);
        let to_t = schema
            .table(&r.to.qualified_table())
            .map_or_else(|| r.to.qualified_table(), Table::qualified_name);
        for (fc, tc) in r.from.cols().into_iter().zip(r.to.cols()) {
            map.entry((from_t.clone(), fc.to_string()))
                .or_default()
                .push((to_t.clone(), tc.to_string()));
        }
    }
    map
}

fn table_refs<'a>(schema: &'a Schema, qname: &str) -> Vec<(&'a Ref, String, String)> {
    schema
        .refs
        .iter()
        .filter_map(|r| {
            let from_t = schema
                .table(&r.from.qualified_table())
                .map_or_else(|| r.from.qualified_table(), Table::qualified_name);
            let to_t = schema
                .table(&r.to.qualified_table())
                .map_or_else(|| r.to.qualified_table(), Table::qualified_name);
            if from_t == qname || to_t == qname {
                Some((r, from_t, to_t))
            } else {
                None
            }
        })
        .collect()
}

fn col_badges(col: &Column, is_fk: bool) -> Vec<&'static str> {
    let mut badges = Vec::new();
    if col.pk {
        badges.push("PK");
    }
    if col.fk || is_fk {
        badges.push("FK");
    }
    if col.unique {
        badges.push("UK");
    }
    if col.not_null {
        badges.push("NN");
    }
    if col.increment {
        badges.push("INC");
    }
    badges
}

/// Generates structured Markdown documentation for `schema`, including a table index,
/// Mermaid `erDiagram`, column/index/relationship details, enums, and `Note`s.
#[must_use]
pub fn to_markdown(schema: &Schema, title: &str) -> String {
    let title = if title.trim().is_empty() {
        "Database Documentation"
    } else {
        title.trim()
    };
    let mut out = String::new();
    let _ = writeln!(out, "# {title}\n");

    if let Some(note) = schema
        .project_note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        for line in note.lines() {
            let _ = writeln!(out, "> {line}");
        }
        out.push('\n');
    }

    let total_cols: usize = schema.tables.iter().map(|t| t.columns.len()).sum();
    let _ = writeln!(
        out,
        "| Tables | Columns | Relationships | Groups | Enums |\n| ---: | ---: | ---: | ---: | ---: |\n| {} | {} | {} | {} | {} |\n",
        schema.tables.len(),
        total_cols,
        schema.refs.len(),
        schema.groups.len(),
        schema.enums.len(),
    );

    // Table Index
    if !schema.tables.is_empty() || !schema.enums.is_empty() {
        out.push_str("## Table Index\n\n");
        let mut grouped: HashSet<String> = HashSet::new();
        for g in &schema.groups {
            let members: Vec<&Table> = g
                .tables
                .iter()
                .filter_map(|name| schema.table(name))
                .collect();
            if members.is_empty() {
                continue;
            }
            let _ = writeln!(out, "### Group: {}\n", g.name);
            for t in members {
                let q = t.qualified_name();
                grouped.insert(q.clone());
                let anchor = format!("table-{}", slug(&q));
                let note_preview = t
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|n| format!(" — {}", n.lines().next().unwrap_or("")))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "- [`{q}`](#{anchor}) ({} cols){note_preview}",
                    t.columns.len()
                );
            }
            out.push('\n');
        }
        let ungrouped: Vec<&Table> = schema
            .tables
            .iter()
            .filter(|t| !grouped.contains(&t.qualified_name()))
            .collect();
        if !ungrouped.is_empty() {
            if !grouped.is_empty() {
                out.push_str("### Ungrouped Tables\n\n");
            }
            for t in ungrouped {
                let q = t.qualified_name();
                let anchor = format!("table-{}", slug(&q));
                let note_preview = t
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|n| format!(" — {}", n.lines().next().unwrap_or("")))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "- [`{q}`](#{anchor}) ({} cols){note_preview}",
                    t.columns.len()
                );
            }
            out.push('\n');
        }
        if !schema.enums.is_empty() {
            out.push_str("### Enums\n\n");
            for e in &schema.enums {
                let q = e.qualified_name();
                let anchor = format!("enum-{}", slug(&q));
                let _ = writeln!(out, "- [`{q}`](#{anchor}) ({} values)", e.values.len());
            }
            out.push('\n');
        }
    }

    // Mermaid ER Diagram
    if !schema.tables.is_empty() {
        out.push_str("## Entity Relationship Diagram\n\n```mermaid\n");
        out.push_str(&to_mermaid(schema));
        out.push_str("\n```\n\n");
    }

    // Tables
    let fk_targets = fk_targets_for(schema);
    if !schema.tables.is_empty() {
        out.push_str("## Tables\n\n");
        for t in &schema.tables {
            let q = t.qualified_name();
            let anchor = format!("table-{}", slug(&q));
            let _ = writeln!(out, "<a id=\"{anchor}\"></a>");
            let _ = writeln!(out, "### `{q}`\n");

            let mut meta = Vec::new();
            if let Some(s) = &t.schema {
                meta.push(format!("**Schema:** `{s}`"));
            }
            if let Some(g) = schema.group_of(&q) {
                meta.push(format!("**Group:** `{}`", g.name));
            }
            if let Some(a) = &t.alias {
                meta.push(format!("**Alias:** `{a}`"));
            }
            if !meta.is_empty() {
                let _ = writeln!(out, "{}\n", meta.join(" · "));
            }

            if let Some(note) = t.note.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                for line in note.lines() {
                    let _ = writeln!(out, "> {line}");
                }
                out.push('\n');
            }

            out.push_str("| Column | Type | Attributes | Default | Note |\n| :--- | :--- | :--- | :--- | :--- |\n");
            for c in &t.columns {
                let targets = fk_targets.get(&(q.clone(), c.name.clone()));
                let mut attrs: Vec<String> = col_badges(c, targets.is_some())
                    .into_iter()
                    .map(|b| format!("`{b}`"))
                    .collect();
                if let Some(chk) = c.check.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    attrs.push(format!("`CHECK({})`", esc_md_cell(chk)));
                }
                if let Some(ts) = targets {
                    for (tt, tc) in ts {
                        attrs.push(format!("→ [`{tt}.{tc}`](#table-{})", slug(tt)));
                    }
                }
                let def = c
                    .default_value
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|d| format!("`{}`", esc_md_cell(d)))
                    .unwrap_or_default();
                let note = c
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(esc_md_cell)
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "| `{}` | `{}` | {} | {} | {} |",
                    esc_md_cell(&c.name),
                    esc_md_cell(&c.ty),
                    attrs.join(" "),
                    def,
                    note
                );
            }
            out.push('\n');

            if !t.indexes.is_empty() {
                out.push_str("**Indexes**\n\n| Columns | Flags | Type | Name |\n| :--- | :--- | :--- | :--- |\n");
                for idx in &t.indexes {
                    let cols: Vec<String> = idx
                        .columns
                        .iter()
                        .map(|ic| format!("`{}`", esc_md_cell(&ic.as_dbml_str())))
                        .collect();
                    let mut flags = Vec::new();
                    if idx.pk {
                        flags.push("`PK`");
                    }
                    if idx.unique {
                        flags.push("`UNIQUE`");
                    }
                    let ty = idx
                        .index_type
                        .map(|ty| format!("`{}`", ty.as_sql_str()))
                        .unwrap_or_default();
                    let name = idx
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(|n| format!("`{}`", esc_md_cell(n)))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        cols.join(", "),
                        flags.join(" "),
                        ty,
                        name
                    );
                }
                out.push('\n');
            }

            let rels = table_refs(schema, &q);
            if !rels.is_empty() {
                out.push_str("**Relationships**\n\n");
                for (r, from_t, to_t) in rels {
                    let fc = r.from.cols().join(", ");
                    let tc = r.to.cols().join(", ");
                    let mut actions = Vec::new();
                    if let Some(d) = r.on_delete {
                        actions.push(format!("ON DELETE {}", d.as_dbml_str()));
                    }
                    if let Some(u) = r.on_update {
                        actions.push(format!("ON UPDATE {}", u.as_dbml_str()));
                    }
                    let act_suffix = if actions.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", actions.join(", "))
                    };
                    let _ = writeln!(
                        out,
                        "- [`{from_t}`](#table-{}).`{fc}` {} [`{to_t}`](#table-{}).`{tc}`{act_suffix}",
                        slug(&from_t),
                        ref_cardinality_label(r.kind),
                        slug(&to_t),
                    );
                }
                out.push('\n');
            }
        }
    }

    // Enums
    if !schema.enums.is_empty() {
        out.push_str("## Enums\n\n");
        for e in &schema.enums {
            write_enum_md(&mut out, e);
        }
    }

    out
}

fn write_enum_md(out: &mut String, e: &EnumDef) {
    let q = e.qualified_name();
    let _ = writeln!(out, "<a id=\"enum-{}\"></a>", slug(&q));
    let _ = writeln!(out, "### `{q}`\n");
    out.push_str("| Value | Note |\n| :--- | :--- |\n");
    for v in &e.values {
        let note = v
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(esc_md_cell)
            .unwrap_or_default();
        let _ = writeln!(out, "| `{}` | {} |", esc_md_cell(&v.name), note);
    }
    out.push('\n');
}

/// Generates a self-contained, zero-dependency static HTML documentation page for `schema`
/// with live search, collapsible group/table sidebar, column/index/note details, and an
/// embedded interactive SVG diagram.
#[must_use]
pub fn to_html(schema: &Schema, svg: &str, title: &str) -> String {
    let title = if title.trim().is_empty() {
        "Database Documentation"
    } else {
        title.trim()
    };
    let fk_targets = fk_targets_for(schema);
    let total_cols: usize = schema.tables.iter().map(|t| t.columns.len()).sum();

    let mut sidebar = String::new();
    let mut grouped: HashSet<String> = HashSet::new();

    for g in &schema.groups {
        let members: Vec<&Table> = g
            .tables
            .iter()
            .filter_map(|name| schema.table(name))
            .collect();
        if members.is_empty() {
            continue;
        }
        let _ = writeln!(
            sidebar,
            "<details class=\"nav-group\" open><summary>{} <span class=\"cnt\">{}</span></summary><ul>",
            esc_html(&g.name),
            members.len()
        );
        for t in members {
            let q = t.qualified_name();
            grouped.insert(q.clone());
            let search_blob = table_search_blob(t);
            let _ = writeln!(
                sidebar,
                "<li data-search=\"{}\"><a href=\"#table-{}\">{}</a></li>",
                esc_html(&search_blob),
                slug(&q),
                esc_html(&q),
            );
        }
        sidebar.push_str("</ul></details>");
    }

    let ungrouped: Vec<&Table> = schema
        .tables
        .iter()
        .filter(|t| !grouped.contains(&t.qualified_name()))
        .collect();
    if !ungrouped.is_empty() {
        let label = if grouped.is_empty() {
            "Tables"
        } else {
            "Ungrouped"
        };
        let _ = writeln!(
            sidebar,
            "<details class=\"nav-group\" open><summary>{label} <span class=\"cnt\">{}</span></summary><ul>",
            ungrouped.len()
        );
        for t in ungrouped {
            let q = t.qualified_name();
            let search_blob = table_search_blob(t);
            let _ = writeln!(
                sidebar,
                "<li data-search=\"{}\"><a href=\"#table-{}\">{}</a></li>",
                esc_html(&search_blob),
                slug(&q),
                esc_html(&q),
            );
        }
        sidebar.push_str("</ul></details>");
    }

    if !schema.enums.is_empty() {
        let _ = writeln!(
            sidebar,
            "<details class=\"nav-group\" open><summary>Enums <span class=\"cnt\">{}</span></summary><ul>",
            schema.enums.len()
        );
        for e in &schema.enums {
            let q = e.qualified_name();
            let vals: Vec<&str> = e.values.iter().map(|v| v.name.as_str()).collect();
            let blob = format!("{q} {}", vals.join(" ")).to_ascii_lowercase();
            let _ = writeln!(
                sidebar,
                "<li data-search=\"{}\"><a href=\"#enum-{}\">{}</a></li>",
                esc_html(&blob),
                slug(&q),
                esc_html(&q),
            );
        }
        sidebar.push_str("</ul></details>");
    }

    let mut body = String::new();

    if let Some(note) = schema
        .project_note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let _ = writeln!(
            body,
            "<div class=\"project-note\">{}</div>",
            esc_html(note).replace('\n', "<br>")
        );
    }

    // Interactive SVG Diagram panel
    if !svg.trim().is_empty() {
        let _ = writeln!(
            body,
            "<section class=\"diagram-card\" id=\"diagram\">\
             <div class=\"diagram-bar\">\
               <span class=\"diagram-title\">ER Diagram</span>\
               <div class=\"diagram-controls\">\
                 <button type=\"button\" data-zoom=\"-1\" title=\"Zoom out\">−</button>\
                 <button type=\"button\" data-zoom=\"0\" title=\"Reset zoom\">100%</button>\
                 <button type=\"button\" data-zoom=\"1\" title=\"Zoom in\">+</button>\
                 <button type=\"button\" id=\"toggle-diagram\" title=\"Collapse/expand\">Hide</button>\
               </div>\
             </div>\
             <div class=\"diagram-stage\" id=\"diagram-stage\">\
               <div class=\"diagram-inner\" id=\"diagram-inner\">{svg}</div>\
             </div>\
             </section>"
        );
    }

    // Tables
    for t in &schema.tables {
        let q = t.qualified_name();
        let search_blob = table_search_blob(t);
        let _ = writeln!(
            body,
            "<article class=\"entity-card\" id=\"table-{}\" data-search=\"{}\">",
            slug(&q),
            esc_html(&search_blob)
        );
        let _ = write!(
            body,
            "<header class=\"entity-head\"><h2>{}</h2><div class=\"meta\">",
            esc_html(&q)
        );
        if let Some(s) = &t.schema {
            let _ = write!(body, "<span class=\"pill\">schema: {}</span>", esc_html(s));
        }
        if let Some(g) = schema.group_of(&q) {
            let _ = write!(
                body,
                "<span class=\"pill\">group: {}</span>",
                esc_html(&g.name)
            );
        }
        if let Some(a) = &t.alias {
            let _ = write!(body, "<span class=\"pill\">as {}</span>", esc_html(a));
        }
        let _ = write!(
            body,
            "<span class=\"pill\">{} cols</span></div></header>",
            t.columns.len()
        );

        if let Some(note) = t.note.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let _ = writeln!(
                body,
                "<p class=\"entity-note\">{}</p>",
                esc_html(note).replace('\n', "<br>")
            );
        }

        body.push_str("<div class=\"tbl-wrap\"><table><thead><tr><th>Column</th><th>Type</th><th>Constraints</th><th>Default</th><th>Note</th></tr></thead><tbody>");
        for c in &t.columns {
            let targets = fk_targets.get(&(q.clone(), c.name.clone()));
            let mut badges_html = String::new();
            for b in col_badges(c, targets.is_some()) {
                let cls = b.to_ascii_lowercase();
                let _ = write!(badges_html, "<span class=\"badge b-{cls}\">{b}</span>");
            }
            if let Some(chk) = c.check.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                let _ = write!(
                    badges_html,
                    "<span class=\"badge b-chk\" title=\"CHECK ({})\">CHK</span>",
                    esc_html(chk)
                );
            }
            if let Some(ts) = targets {
                for (tt, tc) in ts {
                    let _ = write!(
                        badges_html,
                        " <a class=\"fk-link\" href=\"#table-{}\">→ {}.{}</a>",
                        slug(tt),
                        esc_html(tt),
                        esc_html(tc)
                    );
                }
            }
            let def = c
                .default_value
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|d| format!("<code>{}</code>", esc_html(d)))
                .unwrap_or_default();
            let note = c
                .note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|n| esc_html(n).replace('\n', "<br>"))
                .unwrap_or_default();
            let _ = writeln!(
                body,
                "<tr><td class=\"col-name\"><code>{}</code></td><td><code class=\"col-ty\">{}</code></td><td>{}</td><td>{}</td><td class=\"col-note\">{}</td></tr>",
                esc_html(&c.name),
                esc_html(&c.ty),
                badges_html,
                def,
                note
            );
        }
        body.push_str("</tbody></table></div>");

        if !t.indexes.is_empty() {
            body.push_str("<div class=\"sub-sec\"><h3>Indexes</h3><ul class=\"idx-list\">");
            for idx in &t.indexes {
                let cols: Vec<String> = idx
                    .columns
                    .iter()
                    .map(|ic| format!("<code>{}</code>", esc_html(&ic.as_dbml_str())))
                    .collect();
                let mut tags = String::new();
                if idx.pk {
                    tags.push_str("<span class=\"badge b-pk\">PK</span>");
                }
                if idx.unique {
                    tags.push_str("<span class=\"badge b-uk\">UNIQUE</span>");
                }
                if let Some(ty) = idx.index_type {
                    let _ = write!(tags, "<span class=\"pill\">{}</span>", ty.as_sql_str());
                }
                if let Some(name) = idx.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    let _ = write!(tags, "<span class=\"pill\">{}</span>", esc_html(name));
                }
                let _ = writeln!(body, "<li>{} {tags}</li>", cols.join(", "));
            }
            body.push_str("</ul></div>");
        }

        let rels = table_refs(schema, &q);
        if !rels.is_empty() {
            body.push_str("<div class=\"sub-sec\"><h3>Relationships</h3><ul class=\"rel-list\">");
            for (r, from_t, to_t) in rels {
                let fc = r.from.cols().join(", ");
                let tc = r.to.cols().join(", ");
                let mut actions = String::new();
                if let Some(d) = r.on_delete {
                    let _ = write!(
                        actions,
                        " <span class=\"pill\">ON DELETE {}</span>",
                        d.as_dbml_str()
                    );
                }
                if let Some(u) = r.on_update {
                    let _ = write!(
                        actions,
                        " <span class=\"pill\">ON UPDATE {}</span>",
                        u.as_dbml_str()
                    );
                }
                let _ = writeln!(
                    body,
                    "<li><a href=\"#table-{}\"><code>{}.{}</code></a> <span class=\"badge b-fk\">{}</span> <a href=\"#table-{}\"><code>{}.{}</code></a>{actions}</li>",
                    slug(&from_t),
                    esc_html(&from_t),
                    esc_html(&fc),
                    ref_cardinality_badge(r.kind),
                    slug(&to_t),
                    esc_html(&to_t),
                    esc_html(&tc),
                );
            }
            body.push_str("</ul></div>");
        }

        body.push_str("</article>");
    }

    // Enums
    for e in &schema.enums {
        let q = e.qualified_name();
        let vals: Vec<&str> = e.values.iter().map(|v| v.name.as_str()).collect();
        let blob = format!("{q} {}", vals.join(" ")).to_ascii_lowercase();
        let _ = writeln!(
            body,
            "<article class=\"entity-card\" id=\"enum-{}\" data-search=\"{}\">\
             <header class=\"entity-head\"><h2>{}</h2><div class=\"meta\"><span class=\"pill\">ENUM</span><span class=\"pill\">{} values</span></div></header>\
             <div class=\"tbl-wrap\"><table><thead><tr><th>Value</th><th>Note</th></tr></thead><tbody>",
            slug(&q),
            esc_html(&blob),
            esc_html(&q),
            e.values.len()
        );
        for v in &e.values {
            let note = v
                .note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|n| esc_html(n).replace('\n', "<br>"))
                .unwrap_or_default();
            let _ = writeln!(
                body,
                "<tr><td><code>{}</code></td><td class=\"col-note\">{}</td></tr>",
                esc_html(&v.name),
                note
            );
        }
        body.push_str("</tbody></table></div></article>");
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title_esc}</title>
<style>
:root {{
  --bg:#0b0c0e; --surface:#141518; --surface-2:#1c1e22; --border:rgba(255,255,255,.10);
  --text:#ececed; --muted:#9aa0a6; --accent:#c6f240; --link:#6ea8fe; --code-bg:rgba(255,255,255,.06);
}}
[data-theme="light"] {{
  --bg:#f6f7f9; --surface:#ffffff; --surface-2:#f0f2f5; --border:rgba(15,23,42,.12);
  --text:#1f2328; --muted:#57606a; --accent:#5f7d00; --link:#0969da; --code-bg:rgba(15,23,42,.06);
}}
* {{ box-sizing:border-box; }}
html,body {{ margin:0; padding:0; background:var(--bg); color:var(--text); font:14px/1.55 system-ui,-apple-system,Segoe UI,Roboto,sans-serif; }}
a {{ color:var(--link); text-decoration:none; }}
a:hover {{ text-decoration:underline; }}
code {{ font:12.5px/1.4 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; background:var(--code-bg); padding:1px 5px; border-radius:4px; }}
.layout {{ display:flex; min-height:100vh; }}
.sidebar {{ width:270px; flex-shrink:0; background:var(--surface); border-right:1px solid var(--border); padding:16px; position:sticky; top:0; height:100vh; overflow-y:auto; display:flex; flex-direction:column; gap:12px; }}
.brand {{ display:flex; align-items:center; justify-content:space-between; gap:8px; }}
.brand h1 {{ margin:0; font-size:15px; font-weight:700; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
.theme-btn {{ background:var(--surface-2); color:var(--text); border:1px solid var(--border); border-radius:6px; padding:4px 8px; cursor:pointer; font-size:12px; }}
.stats {{ font-size:11.5px; color:var(--muted); }}
#doc-search {{ width:100%; padding:7px 10px; border-radius:8px; border:1px solid var(--border); background:var(--surface-2); color:var(--text); outline:none; font-size:13px; }}
#doc-search:focus {{ border-color:var(--accent); }}
.nav-group {{ margin-bottom:6px; }}
.nav-group summary {{ cursor:pointer; font-weight:600; font-size:12px; text-transform:uppercase; letter-spacing:.04em; color:var(--muted); padding:4px 0; display:flex; align-items:center; justify-content:space-between; }}
.nav-group ul {{ list-style:none; margin:4px 0 0; padding:0; }}
.nav-group li a {{ display:block; padding:4px 8px; border-radius:6px; color:var(--text); font-size:13px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
.nav-group li a:hover {{ background:var(--surface-2); text-decoration:none; }}
.cnt {{ font-size:11px; background:var(--surface-2); padding:1px 6px; border-radius:10px; }}
.main {{ flex:1; min-width:0; padding:24px 32px; max-width:1180px; margin:0 auto; display:flex; flex-direction:column; gap:20px; }}
.project-note,.entity-note {{ margin:0; padding:10px 14px; border-left:3px solid var(--accent); background:var(--surface-2); border-radius:0 8px 8px 0; color:var(--text); }}
.diagram-card,.entity-card {{ background:var(--surface); border:1px solid var(--border); border-radius:12px; overflow:hidden; }}
.diagram-bar,.entity-head {{ display:flex; align-items:center; justify-content:space-between; gap:12px; padding:12px 16px; border-bottom:1px solid var(--border); flex-wrap:wrap; }}
.diagram-title {{ font-weight:600; font-size:13.5px; }}
.diagram-controls {{ display:flex; gap:6px; }}
.diagram-controls button {{ background:var(--surface-2); color:var(--text); border:1px solid var(--border); border-radius:6px; padding:3px 9px; cursor:pointer; font-size:12px; }}
.diagram-stage {{ height:420px; overflow:hidden; position:relative; cursor:grab; background:var(--bg); }}
.diagram-stage.dragging {{ cursor:grabbing; }}
.diagram-stage.collapsed {{ display:none; }}
.diagram-inner {{ transform-origin:0 0; display:inline-block; }}
.diagram-inner svg {{ display:block; max-width:none; height:auto; }}
.entity-head h2 {{ margin:0; font-size:16px; font-weight:650; }}
.meta {{ display:flex; gap:6px; flex-wrap:wrap; }}
.pill {{ font-size:11px; color:var(--muted); background:var(--surface-2); border:1px solid var(--border); padding:2px 7px; border-radius:999px; }}
.entity-note {{ margin:12px 16px 0; }}
.tbl-wrap {{ overflow-x:auto; }}
table {{ width:100%; border-collapse:collapse; font-size:13px; }}
th,td {{ text-align:left; padding:8px 14px; border-bottom:1px solid var(--border); vertical-align:top; }}
th {{ font-size:11px; text-transform:uppercase; letter-spacing:.04em; color:var(--muted); background:var(--surface-2); }}
tbody tr:last-child td {{ border-bottom:none; }}
.col-note {{ color:var(--muted); max-width:340px; }}
.badge {{ display:inline-block; font:600 10px/1.2 ui-monospace,monospace; padding:2px 5px; border-radius:4px; margin-right:4px; }}
.b-pk {{ background:rgba(198,242,64,.18); color:var(--accent); }}
.b-fk {{ background:rgba(110,168,254,.18); color:var(--link); }}
.b-uk,.b-nn,.b-inc,.b-chk {{ background:var(--surface-2); color:var(--muted); border:1px solid var(--border); }}
.fk-link {{ font-size:12px; white-space:nowrap; }}
.sub-sec {{ padding:10px 16px 14px; border-top:1px solid var(--border); }}
.sub-sec h3 {{ margin:0 0 8px; font-size:12px; text-transform:uppercase; letter-spacing:.04em; color:var(--muted); }}
.idx-list,.rel-list {{ margin:0; padding-left:18px; display:flex; flex-direction:column; gap:5px; font-size:13px; }}
.hidden {{ display:none !important; }}
@media (max-width:860px) {{
  .layout {{ flex-direction:column; }}
  .sidebar {{ width:100%; height:auto; position:static; }}
  .main {{ padding:16px; }}
}}
</style>
</head>
<body>
<div class="layout">
  <aside class="sidebar">
    <div class="brand">
      <h1 title="{title_esc}">{title_esc}</h1>
      <button type="button" class="theme-btn" id="theme-toggle">Light</button>
    </div>
    <div class="stats">{tables_cnt} tables · {cols_cnt} cols · {refs_cnt} refs · {enums_cnt} enums</div>
    <input id="doc-search" type="search" placeholder="Search tables, columns, enums…" autocomplete="off" />
    <nav id="doc-nav">{sidebar}</nav>
  </aside>
  <main class="main">{body}</main>
</div>
<script>
(function(){{
  var root = document.documentElement;
  var themeBtn = document.getElementById('theme-toggle');
  if (themeBtn) {{
    themeBtn.addEventListener('click', function() {{
      var next = root.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
      root.setAttribute('data-theme', next);
      themeBtn.textContent = next === 'dark' ? 'Light' : 'Dark';
    }});
  }}
  var search = document.getElementById('doc-search');
  if (search) {{
    search.addEventListener('input', function() {{
      var q = search.value.trim().toLowerCase();
      document.querySelectorAll('[data-search]').forEach(function(el) {{
        var hay = el.getAttribute('data-search') || '';
        el.classList.toggle('hidden', q !== '' && hay.indexOf(q) === -1);
      }});
    }});
  }}
  var stage = document.getElementById('diagram-stage');
  var inner = document.getElementById('diagram-inner');
  var toggleBtn = document.getElementById('toggle-diagram');
  if (toggleBtn && stage) {{
    toggleBtn.addEventListener('click', function() {{
      var c = stage.classList.toggle('collapsed');
      toggleBtn.textContent = c ? 'Show' : 'Hide';
    }});
  }}
  if (stage && inner) {{
    var scale = 1, tx = 16, ty = 16, drag = false, sx = 0, sy = 0;
    var apply = function() {{ inner.style.transform = 'translate(' + tx + 'px,' + ty + 'px) scale(' + scale + ')'; }};
    apply();
    document.querySelectorAll('[data-zoom]').forEach(function(btn) {{
      btn.addEventListener('click', function() {{
        var d = Number(btn.getAttribute('data-zoom'));
        if (d === 0) {{ scale = 1; tx = 16; ty = 16; }}
        else {{ scale = Math.min(3, Math.max(0.2, scale * (d > 0 ? 1.2 : 1 / 1.2))); }}
        apply();
      }});
    }});
    stage.addEventListener('wheel', function(e) {{
      e.preventDefault();
      var factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
      scale = Math.min(3, Math.max(0.2, scale * factor));
      apply();
    }}, {{ passive: false }});
    stage.addEventListener('mousedown', function(e) {{ drag = true; sx = e.clientX - tx; sy = e.clientY - ty; stage.classList.add('dragging'); }});
    window.addEventListener('mousemove', function(e) {{ if (!drag) return; tx = e.clientX - sx; ty = e.clientY - sy; apply(); }});
    window.addEventListener('mouseup', function() {{ drag = false; stage.classList.remove('dragging'); }});
  }}
}})();
</script>
</body>
</html>
"#,
        title_esc = esc_html(title),
        tables_cnt = schema.tables.len(),
        cols_cnt = total_cols,
        refs_cnt = schema.refs.len(),
        enums_cnt = schema.enums.len(),
        sidebar = sidebar,
        body = body,
    )
}

fn table_search_blob(t: &Table) -> String {
    let mut parts = vec![t.qualified_name()];
    if let Some(note) = &t.note {
        parts.push(note.clone());
    }
    for c in &t.columns {
        parts.push(c.name.clone());
        parts.push(c.ty.clone());
        if let Some(n) = &c.note {
            parts.push(n.clone());
        }
    }
    parts.join(" ").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{EnumValue, Group, IndexColumn, IndexDef, IndexType, RefAction};

    fn sample_schema() -> Schema {
        Schema {
            project_note: Some("Core e-commerce schema".into()),
            tables: vec![
                Table {
                    name: "users".into(),
                    note: Some("Registered accounts".into()),
                    columns: vec![
                        Column {
                            name: "id".into(),
                            ty: "uuid".into(),
                            pk: true,
                            not_null: true,
                            ..Default::default()
                        },
                        Column {
                            name: "email".into(),
                            ty: "varchar(255)".into(),
                            unique: true,
                            not_null: true,
                            note: Some("Primary login <email>".into()),
                            ..Default::default()
                        },
                    ],
                    indexes: vec![IndexDef {
                        columns: vec![IndexColumn::Column("email".into())],
                        unique: true,
                        pk: false,
                        name: Some("uq_users_email".into()),
                        index_type: Some(IndexType::BTree),
                    }],
                    ..Default::default()
                },
                Table {
                    name: "orders".into(),
                    columns: vec![
                        Column {
                            name: "id".into(),
                            ty: "int".into(),
                            pk: true,
                            ..Default::default()
                        },
                        Column {
                            name: "user_id".into(),
                            ty: "uuid".into(),
                            fk: true,
                            not_null: true,
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            refs: vec![Ref {
                from: schema::Endpoint {
                    table: "orders".into(),
                    column: "user_id".into(),
                    ..Default::default()
                },
                to: schema::Endpoint {
                    table: "users".into(),
                    column: "id".into(),
                    ..Default::default()
                },
                kind: RefKind::ManyToOne,
                on_delete: Some(RefAction::Cascade),
                on_update: None,
            }],
            groups: vec![Group {
                name: "core".into(),
                tables: vec!["users".into(), "orders".into()],
                color: None,
            }],
            enums: vec![EnumDef {
                schema: None,
                name: "order_status".into(),
                values: vec![EnumValue {
                    name: "paid".into(),
                    note: Some("Settled".into()),
                }],
            }],
        }
    }

    #[test]
    fn markdown_includes_index_mermaid_notes_indexes_refs_and_enums() {
        let s = sample_schema();
        let md = to_markdown(&s, "Shop Docs");
        assert!(md.starts_with("# Shop Docs\n"));
        assert!(md.contains("> Core e-commerce schema"));
        assert!(md.contains("## Table Index"));
        assert!(md.contains("```mermaid\nerDiagram"));
        assert!(md.contains("> Registered accounts"));
        assert!(md.contains("Primary login <email>"));
        assert!(md.contains("uq_users_email"));
        assert!(md.contains("ON DELETE cascade"));
        assert!(md.contains("## Enums"));
        assert!(md.contains("| `paid` | Settled |"));
        // Deterministic
        assert_eq!(md, to_markdown(&s, "Shop Docs"));
    }

    #[test]
    fn html_is_self_contained_escapes_notes_and_embeds_svg() {
        let s = sample_schema();
        let html = to_html(&s, "<svg id=\"er\"></svg>", "Shop <Docs>");
        assert!(html.contains("<title>Shop &lt;Docs&gt;</title>"));
        assert!(html.contains("Primary login &lt;email&gt;"));
        assert!(html.contains("<svg id=\"er\"></svg>"));
        assert!(html.contains("id=\"doc-search\""));
        assert!(!html.contains("https://"));
        assert!(!html.contains("http://"));
        assert_eq!(html, to_html(&s, "<svg id=\"er\"></svg>", "Shop <Docs>"));
    }
}
