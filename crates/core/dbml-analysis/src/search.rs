//! Case-insensitive schema search index over tables, columns (and types), and enums.

use schema::Schema;
use serde::{Deserialize, Serialize};

/// Target entity matched by [`search_schema`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchKind {
    /// A database table.
    Table {
        /// Qualified table name (`schema.table` or `table`).
        table: String,
        /// Optional schema qualifier.
        schema: Option<String>,
    },
    /// A column belonging to a table.
    Column {
        /// Qualified parent table name.
        table: String,
        /// Column name.
        column: String,
        /// Column data type.
        ty: String,
    },
    /// A schema enum definition.
    Enum {
        /// Qualified enum name.
        name: String,
        /// Number of enum variants.
        values_count: usize,
    },
}

/// Ranked search result returned by [`search_schema`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Target entity metadata.
    pub kind: SearchKind,
    /// Primary display label (e.g. `users`, `order_items.tax_amount`, `order_status`).
    pub label: String,
    /// Secondary detail shown next to the label (e.g. column count, column type, variant count).
    pub detail: String,
    /// Match rank: `0` exact, `1` prefix, `2` substring, `3` fuzzy subsequence.
    pub score: u8,
}

/// Scores `candidate` against lowercase `query`:
/// `0` = exact, `1` = prefix, `2` = substring, `3` = fuzzy subsequence.
fn match_score(candidate: &str, query_lower: &str) -> Option<u8> {
    let cand_lower = candidate.to_ascii_lowercase();
    if cand_lower == query_lower {
        return Some(0);
    }
    if cand_lower.starts_with(query_lower) {
        return Some(1);
    }
    if cand_lower.contains(query_lower) {
        return Some(2);
    }
    // Fuzzy subsequence check
    let mut qi = query_lower.chars().peekable();
    for ch in cand_lower.chars() {
        if qi.peek().copied() == Some(ch) {
            qi.next();
            if qi.peek().is_none() {
                return Some(3);
            }
        }
    }
    None
}

fn kind_rank(kind: &SearchKind) -> u8 {
    match kind {
        SearchKind::Table { .. } => 0,
        SearchKind::Enum { .. } => 1,
        SearchKind::Column { .. } => 2,
    }
}

/// Searches tables, columns (names, `table.column`, and types), and enums in `schema`.
///
/// Results are ranked exact (`0`) > prefix (`1`) > substring (`2`) > fuzzy (`3`).
/// When `query` is empty/whitespace, returns all tables and enums in declaration order.
#[must_use]
pub fn search_schema(schema: &Schema, query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        let mut hits = Vec::with_capacity(schema.tables.len() + schema.enums.len());
        for t in &schema.tables {
            let qname = t.qualified_name();
            hits.push(SearchHit {
                kind: SearchKind::Table {
                    table: qname.clone(),
                    schema: t.schema.clone(),
                },
                label: qname,
                detail: format!("{} cols", t.columns.len()),
                score: 0,
            });
        }
        for e in &schema.enums {
            let qname = e.qualified_name();
            hits.push(SearchHit {
                kind: SearchKind::Enum {
                    name: qname.clone(),
                    values_count: e.values.len(),
                },
                label: qname,
                detail: format!("{} values", e.values.len()),
                score: 0,
            });
        }
        return hits;
    }

    let mut ranked: Vec<(u8, u8, SearchHit)> = Vec::new();

    for t in &schema.tables {
        let qname = t.qualified_name();
        let best = [match_score(&t.name, &q), match_score(&qname, &q)]
            .into_iter()
            .flatten()
            .min();
        if let Some(score) = best {
            let hit = SearchHit {
                kind: SearchKind::Table {
                    table: qname.clone(),
                    schema: t.schema.clone(),
                },
                label: qname.clone(),
                detail: format!("{} cols", t.columns.len()),
                score,
            };
            ranked.push((score, 0, hit));
        }

        for c in &t.columns {
            let full = format!("{qname}.{}", c.name);
            let name_score = [match_score(&c.name, &q), match_score(&full, &q)]
                .into_iter()
                .flatten()
                .min();
            let type_score = match_score(&c.ty, &q).map(|s| s.saturating_add(1));
            if let Some(score) = [name_score, type_score].into_iter().flatten().min() {
                let is_type_only = name_score.is_none();
                let hit = SearchHit {
                    kind: SearchKind::Column {
                        table: qname.clone(),
                        column: c.name.clone(),
                        ty: c.ty.clone(),
                    },
                    label: full,
                    detail: c.ty.clone(),
                    score: score.min(3),
                };
                ranked.push((score.min(3), u8::from(is_type_only), hit));
            }
        }
    }

    for e in &schema.enums {
        let qname = e.qualified_name();
        let best = [match_score(&e.name, &q), match_score(&qname, &q)]
            .into_iter()
            .flatten()
            .min();
        if let Some(score) = best {
            let hit = SearchHit {
                kind: SearchKind::Enum {
                    name: qname.clone(),
                    values_count: e.values.len(),
                },
                label: qname,
                detail: format!("{} values", e.values.len()),
                score,
            };
            ranked.push((score, 0, hit));
        }
    }

    ranked.sort_by(|(sa, ta, ha), (sb, tb, hb)| {
        sa.cmp(sb)
            .then_with(|| ta.cmp(tb))
            .then_with(|| kind_rank(&ha.kind).cmp(&kind_rank(&hb.kind)))
            .then_with(|| {
                ha.label
                    .to_ascii_lowercase()
                    .cmp(&hb.label.to_ascii_lowercase())
            })
    });

    ranked.into_iter().map(|(_, _, h)| h).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;

    const SAMPLE: &str = r"
Enum order_status {
  pending
  paid
}

Table users {
  id uuid [pk]
  email varchar(255)
}

Table orders {
  id int [pk]
  user_id uuid
  status order_status
}

Table order_items {
  id int [pk]
  order_id int
  tax_amount decimal(10, 2)
}
";

    #[test]
    fn empty_query_returns_tables_and_enums() {
        let schema = analyze(SAMPLE);
        let hits = search_schema(&schema, "   ");
        let labels: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(labels, ["users", "orders", "order_items", "order_status"]);
    }

    #[test]
    fn ranks_exact_above_prefix_above_substring() {
        let schema = analyze(SAMPLE);
        let hits = search_schema(&schema, "orders");
        assert_eq!(hits[0].label, "orders");
        assert_eq!(hits[0].score, 0);

        let prefix_hits = search_schema(&schema, "order");
        let top_labels: Vec<&str> = prefix_hits
            .iter()
            .take(3)
            .map(|h| h.label.as_str())
            .collect();
        assert_eq!(top_labels, ["order_items", "orders", "order_status"]);
    }

    #[test]
    fn finds_columns_by_name_and_by_type() {
        let schema = analyze(SAMPLE);
        let by_col = search_schema(&schema, "tax_amount");
        assert_eq!(by_col.len(), 1);
        assert_eq!(by_col[0].label, "order_items.tax_amount");
        assert_eq!(by_col[0].detail, "decimal(10,2)");

        let by_type = search_schema(&schema, "uuid");
        let labels: Vec<&str> = by_type.iter().map(|h| h.label.as_str()).collect();
        assert!(labels.contains(&"users.id"), "{labels:?}");
        assert!(labels.contains(&"orders.user_id"), "{labels:?}");
    }

    #[test]
    fn supports_fuzzy_subsequence_matching() {
        let schema = analyze(SAMPLE);
        let hits = search_schema(&schema, "txamt");
        assert!(
            hits.iter().any(|h| h.label == "order_items.tax_amount"),
            "{hits:?}"
        );
    }
}
