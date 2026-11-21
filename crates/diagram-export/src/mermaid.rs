use schema::{RefKind, Schema};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Normalises `s` to a Mermaid identifier (`[A-Za-z0-9_]`); distinct inputs may collide.
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

/// Where an identifier collision happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionScope {
    /// Two tables normalise to the same entity name.
    Entity,
    /// Two columns of the same table normalise to the same attribute name.
    Attribute,
}

/// Two distinct names that normalise to the same Mermaid identifier; the later one got a suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentCollision {
    /// Whether it is a table or a column collision.
    pub scope: CollisionScope,
    /// Original (pre-normalisation) name that had to be renamed.
    pub original: String,
    /// Unique identifier emitted for it.
    pub assigned: String,
}

/// Result of [`to_mermaid_checked`]: the diagram and the identifier collisions it resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidExport {
    /// The `erDiagram` text (collisions are also noted as `%%` comments).
    pub text: String,
    /// Names renamed to keep identifiers unique.
    pub collisions: Vec<IdentCollision>,
}

/// Hands out unique normalised identifiers: the same original always maps to the same id.
struct Namer {
    taken: HashSet<String>,
    assigned: HashMap<String, String>,
}

impl Namer {
    fn new() -> Self {
        Self {
            taken: HashSet::new(),
            assigned: HashMap::new(),
        }
    }

    /// Returns the unique id for `original`, plus whether it had to be disambiguated.
    fn id(&mut self, original: &str) -> (String, bool) {
        if let Some(id) = self.assigned.get(original) {
            return (id.clone(), false);
        }
        let base = ident(original);
        let mut candidate = base.clone();
        let mut n = 2;
        while self.taken.contains(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        let renamed = candidate != base;
        self.taken.insert(candidate.clone());
        self.assigned
            .insert(original.to_string(), candidate.clone());
        (candidate, renamed)
    }
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
///
/// Names that normalise to the same identifier get a numeric suffix and a `%%` comment;
/// use [`to_mermaid_checked`] to get the collisions as data.
#[must_use]
pub fn to_mermaid(schema: &Schema) -> String {
    to_mermaid_checked(schema).text
}

/// Like [`to_mermaid`], but also reports every identifier collision it had to resolve.
#[must_use]
pub fn to_mermaid_checked(schema: &Schema) -> MermaidExport {
    let mut out = String::from("erDiagram");
    let mut collisions = Vec::new();
    let mut entities = Namer::new();
    for t in &schema.tables {
        let qualified = t.qualified_name();
        let (entity, renamed) = entities.id(&qualified);
        if renamed {
            collisions.push(IdentCollision {
                scope: CollisionScope::Entity,
                original: qualified,
                assigned: entity.clone(),
            });
        }
        let _ = write!(out, "\n    {entity} {{");
        let mut attributes = Namer::new();
        for c in &t.columns {
            let ty = if c.ty.is_empty() { "string" } else { &c.ty };
            let keys = match (c.pk, c.unique) {
                (true, _) => " PK",
                (false, true) => " UK",
                _ => "",
            };
            let (name, renamed) = attributes.id(&c.name);
            if renamed {
                collisions.push(IdentCollision {
                    scope: CollisionScope::Attribute,
                    original: format!("{}.{}", t.name, c.name),
                    assigned: name.clone(),
                });
            }
            let _ = write!(out, "\n        {} {name}{keys}", ident(ty));
        }
        out.push_str("\n    }");
    }
    for r in &schema.refs {
        let (from, _) = entities.id(&r.from.qualified_table());
        let (to, _) = entities.id(&r.to.qualified_table());
        let _ = write!(
            out,
            "\n    {from} {} {to} : \"{} to {}\"",
            cardinality(r.kind),
            r.from.column,
            r.to.column
        );
    }
    for c in &collisions {
        let what = match c.scope {
            CollisionScope::Entity => "table",
            CollisionScope::Attribute => "column",
        };
        let _ = write!(
            out,
            "\n    %% name collision: {what} `{}` exported as `{}`",
            c.original.replace(['\n', '\r'], " "),
            c.assigned
        );
    }
    MermaidExport {
        text: out,
        collisions,
    }
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

    fn table(name: &str, cols: &[&str]) -> schema::Table {
        schema::Table {
            name: name.into(),
            columns: cols
                .iter()
                .map(|c| schema::Column {
                    name: (*c).into(),
                    ty: "int".into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn colliding_table_names_get_unique_ids_and_are_reported() {
        let s = Schema {
            tables: vec![
                table("a-b", &["id"]),
                table("a_b", &["id"]),
                table("a b", &["id"]),
            ],
            ..Default::default()
        };
        let out = to_mermaid_checked(&s);
        let entities: Vec<_> = out
            .text
            .lines()
            .filter(|l| l.ends_with(" {"))
            .map(str::trim)
            .collect();
        assert_eq!(entities, ["a_b {", "a_b_2 {", "a_b_3 {"], "{}", out.text);
        assert_eq!(out.collisions.len(), 2);
        assert_eq!(out.collisions[0].original, "a_b");
        assert_eq!(out.collisions[0].assigned, "a_b_2");
        assert!(out.text.contains("%% name collision"), "{}", out.text);
    }

    #[test]
    fn colliding_columns_are_disambiguated_per_table() {
        let s = Schema {
            tables: vec![table("t", &["x-y", "x_y"]), table("u", &["x-y"])],
            ..Default::default()
        };
        let out = to_mermaid_checked(&s);
        assert!(
            out.text.contains("int x_y\n        int x_y_2\n"),
            "{}",
            out.text
        );
        assert_eq!(out.collisions.len(), 1);
        assert_eq!(out.collisions[0].scope, CollisionScope::Attribute);
    }

    #[test]
    fn refs_follow_the_disambiguated_entity() {
        let ep = |t: &str| schema::Endpoint {
            table: t.into(),
            column: "id".into(),
            ..Default::default()
        };
        let s = Schema {
            tables: vec![table("a-b", &["id"]), table("a_b", &["id"])],
            refs: vec![schema::Ref {
                from: ep("a_b"),
                to: ep("a-b"),
                kind: RefKind::ManyToOne,
                on_delete: None,
                on_update: None,
            }],
            ..Default::default()
        };
        let out = to_mermaid_checked(&s);
        assert!(
            out.text.contains("a_b_2 }o--|| a_b : \"id to id\""),
            "{}",
            out.text
        );
    }

    #[test]
    fn schema_qualified_ref_matches_its_entity() {
        let mut t = table("users", &["id"]);
        t.schema = Some("auth".into());
        let ep = |tbl: &str, sch: Option<&str>| schema::Endpoint {
            schema: sch.map(Into::into),
            table: tbl.into(),
            column: "id".into(),
            ..Default::default()
        };
        let s = Schema {
            tables: vec![t],
            refs: vec![schema::Ref {
                from: ep("users", Some("auth")),
                to: ep("users", Some("auth")),
                kind: RefKind::OneToOne,
                on_delete: None,
                on_update: None,
            }],
            ..Default::default()
        };
        let out = to_mermaid_checked(&s);
        assert!(out.text.contains("auth_users {"), "{}", out.text);
        assert!(out.text.contains("auth_users ||--|| auth_users"));
    }

    #[test]
    fn clean_schema_reports_no_collisions() {
        let s = Schema {
            tables: vec![table("a", &["id"]), table("b", &["id"])],
            ..Default::default()
        };
        let out = to_mermaid_checked(&s);
        assert_eq!(out.collisions, []);
        assert!(!out.text.contains("%%"));
    }
}
