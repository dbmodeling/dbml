use schema::{RefKind, Schema};
use std::fmt::Write as _;

fn ident(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn cardinality(kind: RefKind) -> &'static str {
    match kind {
        RefKind::ManyToOne => "}o--||",
        RefKind::OneToMany => "||--o{",
        RefKind::OneToOne => "||--||",
        RefKind::ManyToMany => "}o--o{",
    }
}

/// Renders `schema` as a Mermaid `erDiagram` (same output as the web `exportToMermaid`).
#[must_use]
pub fn to_mermaid(schema: &Schema) -> String {
    let mut out = String::from("erDiagram");
    for t in &schema.tables {
        let _ = write!(out, "\n    {} {{", ident(&t.name));
        for c in &t.columns {
            let ty = if c.ty.is_empty() { "string" } else { &c.ty };
            let keys = match (c.pk, c.unique) {
                (true, _) => " PK",
                (false, true) => " UK",
                _ => "",
            };
            let _ = write!(out, "\n        {} {}{keys}", ident(ty), ident(&c.name));
        }
        out.push_str("\n    }");
    }
    for r in &schema.refs {
        let _ = write!(
            out,
            "\n    {} {} {} : \"{} to {}\"",
            ident(&r.from.qualified_table()),
            cardinality(r.kind),
            ident(&r.to.qualified_table()),
            r.from.column,
            r.to.column
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_should_list_tables_keys_and_refs() {
        let s = Schema {
            tables: vec![schema::Table {
                name: "user-s".into(),
                columns: vec![
                    schema::Column {
                        name: "id".into(),
                        ty: "int".into(),
                        pk: true,
                        ..Default::default()
                    },
                    schema::Column {
                        name: "mail".into(),
                        ty: "varchar(20)".into(),
                        unique: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let m = to_mermaid(&s);
        assert_eq!(
            m,
            "erDiagram\n    user_s {\n        int id PK\n        varchar_20_ mail UK\n    }"
        );
    }
}
