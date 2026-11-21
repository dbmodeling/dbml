#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Pure semantic database schema model (tables, columns, relationships, and groups),
//! derived as a projection from the lossless DBML CST.

pub mod diff;

pub use diff::{
    Change, DiffSummary, EnumChange, EnumModified, SchemaDiff, TableChange, TableModified,
    TableProps, TablePropsChange, diff_schemas,
};

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Subgraph returned by [`Schema::neighborhood`]: tables and connecting refs within `depth` hops.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FocusSubgraph {
    /// Qualified names of the seed tables and all tables reachable within `depth` FK hops.
    pub tables: HashSet<String>,
    /// Indices into [`Schema::refs`] of relationships traversed within `depth` hops.
    pub ref_indices: HashSet<usize>,
}

/// Semantic representation of a DBML schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Schema {
    /// Tables defined in the schema.
    pub tables: Vec<Table>,
    /// Foreign-key relationships between columns.
    pub refs: Vec<Ref>,
    /// Named `TableGroup` containers grouping related tables.
    pub groups: Vec<Group>,
    /// Named `Enum` definitions declared in the schema.
    #[serde(default)]
    pub enums: Vec<EnumDef>,
    /// Optional project-level `Note`.
    #[serde(default)]
    pub project_note: Option<String>,
}

/// An `Enum` definition (`Enum [schema.]name { ... }`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EnumDef {
    /// Optional schema qualifier (`schema.enum_name`).
    #[serde(default)]
    pub schema: Option<String>,
    /// Enum identifier.
    pub name: String,
    /// Ordered variants of the enum.
    pub values: Vec<EnumValue>,
}

impl EnumDef {
    /// Returns `schema.name` when qualified, or `name` otherwise.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format_qualified(self.schema.as_deref(), &self.name)
    }
}

fn format_qualified(schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(s) if !s.is_empty() => format!("{s}.{name}"),
        _ => name.to_owned(),
    }
}

/// A single variant inside an [`EnumDef`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EnumValue {
    /// Variant identifier.
    pub name: String,
    /// Optional `[note: '...']` attached to this variant.
    #[serde(default)]
    pub note: Option<String>,
}

/// Index algorithm type in DBML (`[type: btree | hash | gin | gist]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexType {
    /// B-tree index (`btree`).
    BTree,
    /// Hash index (`hash`).
    Hash,
    /// Generalized inverted index (`gin`).
    Gin,
    /// Generalized search tree (`gist`).
    Gist,
}

impl IndexType {
    /// All supported index algorithms in canonical UI order.
    pub const ALL: &[Self] = &[Self::BTree, Self::Hash, Self::Gin, Self::Gist];

    /// Returns the canonical lowercase SQL index method name.
    #[must_use]
    pub const fn as_sql_str(self) -> &'static str {
        match self {
            Self::BTree => "btree",
            Self::Hash => "hash",
            Self::Gin => "gin",
            Self::Gist => "gist",
        }
    }

    /// Parses a DBML index type string (case-insensitive, trimmed).
    #[must_use]
    pub fn from_dbml_str(s: &str) -> Option<Self> {
        let normalized = s.trim();
        if normalized.eq_ignore_ascii_case("btree") {
            Some(Self::BTree)
        } else if normalized.eq_ignore_ascii_case("hash") {
            Some(Self::Hash)
        } else if normalized.eq_ignore_ascii_case("gin") {
            Some(Self::Gin)
        } else if normalized.eq_ignore_ascii_case("gist") {
            Some(Self::Gist)
        } else {
            None
        }
    }
}

/// Item inside an `indexes { ... }` entry (plain column or backtick expression).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum IndexColumn {
    /// Plain column identifier.
    Column(String),
    /// Backtick-quoted SQL expression (e.g. `` `lower(email)` ``).
    Expression(String),
}

impl IndexColumn {
    /// Formats this item for a DBML `indexes { ... }` line.
    #[must_use]
    pub fn as_dbml_str(&self) -> String {
        match self {
            Self::Column(c) => {
                if c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && !c.is_empty() {
                    c.clone()
                } else {
                    format!("\"{c}\"")
                }
            }
            Self::Expression(e) => format!("`{e}`"),
        }
    }

    /// Returns the column identifier when this item is [`Self::Column`].
    #[must_use]
    pub fn column_name(&self) -> Option<&str> {
        match self {
            Self::Column(c) => Some(c.as_str()),
            Self::Expression(_) => None,
        }
    }
}

/// Index or composite primary-key definition inside a `Table`'s `indexes { ... }` block.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IndexDef {
    /// Indexed columns or expressions.
    pub columns: Vec<IndexColumn>,
    /// Whether `[unique]` is set.
    #[serde(default)]
    pub unique: bool,
    /// Whether `[pk]` is set (composite primary key).
    #[serde(default)]
    pub pk: bool,
    /// Optional explicit index name (`[name: '...']`).
    #[serde(default)]
    pub name: Option<String>,
    /// Optional index algorithm (`[type: ...]`).
    #[serde(default)]
    pub index_type: Option<IndexType>,
}

impl IndexDef {
    /// Returns `true` if this index references the plain column `col`.
    #[must_use]
    pub fn references_column(&self, col: &str) -> bool {
        self.columns.iter().any(|ic| ic.column_name() == Some(col))
    }

    /// Formats this index definition as a canonical single-line DBML entry inside `indexes { ... }`.
    #[must_use]
    pub fn to_dbml_line(&self) -> String {
        let cols_part = match self.columns.as_slice() {
            [] => String::new(),
            [single] => single.as_dbml_str(),
            many => {
                let joined: Vec<String> = many.iter().map(IndexColumn::as_dbml_str).collect();
                format!("({})", joined.join(", "))
            }
        };
        let mut settings: Vec<String> = Vec::new();
        if self.pk {
            settings.push("pk".to_string());
        }
        if self.unique {
            settings.push("unique".to_string());
        }
        if let Some(ty) = self.index_type {
            settings.push(format!("type: {}", ty.as_sql_str()));
        }
        if let Some(name) = self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            settings.push(format!("name: '{name}'"));
        }
        if settings.is_empty() {
            cols_part
        } else {
            format!("{cols_part} [{}]", settings.join(", "))
        }
    }
}

/// Explicit nullability state of a DBML [`Column`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nullability {
    /// Neither `[not null]` nor `[null]` was specified.
    #[default]
    Unspecified,
    /// `[not null]` was specified.
    NotNull,
    /// `[null]` was explicitly specified.
    Nullable,
}

impl Nullability {
    /// Returns `true` when `[not null]` was specified.
    #[must_use]
    pub const fn is_not_null(self) -> bool {
        matches!(self, Self::NotNull)
    }
}

/// A `TableGroup name [color: #hex] { table ... }` container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// Group identifier.
    pub name: String,
    /// Names of tables belonging to this group.
    pub tables: Vec<String>,
    /// Optional hex header/group color (`[color: #hex]`).
    pub color: Option<String>,
}

/// A database table definition.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Table {
    /// Table name.
    pub name: String,
    /// Ordered list of columns in this table.
    pub columns: Vec<Column>,
    /// Optional hex header color (`[headercolor: #hex]`).
    pub color: Option<String>,
    /// Optional database schema qualifier (`schema.table`).
    #[serde(default)]
    pub schema: Option<String>,
    /// Optional DBML table alias (`Table users as u`).
    #[serde(default)]
    pub alias: Option<String>,
    /// Optional table-level `Note` (`Note: '...'` or `[note: '...']`).
    #[serde(default)]
    pub note: Option<String>,
    /// Indexes declared inside `indexes { ... }`.
    #[serde(default)]
    pub indexes: Vec<IndexDef>,
}

impl Table {
    /// Returns `schema.name` when qualified, or `name` otherwise.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format_qualified(self.schema.as_deref(), &self.name)
    }
}

/// A column inside a [`Table`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Column {
    /// Column name.
    pub name: String,
    /// Column data type (e.g., `int`, `varchar(255)`).
    pub ty: String,
    /// Whether this column is part of the primary key (`[pk]`).
    pub pk: bool,
    /// Whether this column participates in a foreign key relationship.
    #[serde(default)]
    pub fk: bool,
    /// Whether this column has a `not null` constraint (`[not null]`).
    pub not_null: bool,
    /// Whether this column has a `unique` constraint (`[unique]`).
    pub unique: bool,
    /// Optional `[default: ...]` expression or literal.
    #[serde(default)]
    pub default_value: Option<String>,
    /// Whether `[increment]` is set.
    #[serde(default)]
    pub increment: bool,
    /// Explicit nullability (`[not null]`, `[null]`, or unspecified).
    #[serde(default)]
    pub nullability: Nullability,
    /// Optional `[note: '...']` attached to this column.
    #[serde(default)]
    pub note: Option<String>,
    /// Optional `[check: ...]` constraint expression attached to this column.
    #[serde(default)]
    pub check: Option<String>,
}

/// Directed relationship edge between two columns.
///
/// `kind` follows the DBML arrow operator (`>` many-to-one, `<` one-to-many, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    /// Source endpoint of the relationship.
    pub from: Endpoint,
    /// Target endpoint of the relationship.
    pub to: Endpoint,
    /// Relationship cardinality.
    pub kind: RefKind,
    /// Optional `ON DELETE` referential action.
    #[serde(default)]
    pub on_delete: Option<RefAction>,
    /// Optional `ON UPDATE` referential action.
    #[serde(default)]
    pub on_update: Option<RefAction>,
}

/// Referential integrity action for `ON DELETE` / `ON UPDATE` on a [`Ref`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefAction {
    /// `no action`
    NoAction,
    /// `restrict`
    Restrict,
    /// `cascade`
    Cascade,
    /// `set null`
    SetNull,
    /// `set default`
    SetDefault,
}

impl RefAction {
    /// All supported referential actions in canonical UI order.
    pub const ALL: &[Self] = &[
        Self::NoAction,
        Self::Restrict,
        Self::Cascade,
        Self::SetNull,
        Self::SetDefault,
    ];

    /// Returns the canonical DBML keyword string for this action.
    #[must_use]
    pub const fn as_dbml_str(self) -> &'static str {
        match self {
            Self::NoAction => "no action",
            Self::Restrict => "restrict",
            Self::Cascade => "cascade",
            Self::SetNull => "set null",
            Self::SetDefault => "set default",
        }
    }

    /// Parses a DBML referential action string (case-insensitive, trimmed).
    #[must_use]
    pub fn from_dbml_str(s: &str) -> Option<Self> {
        let normalized = s.trim();
        if normalized.eq_ignore_ascii_case("no action") {
            Some(Self::NoAction)
        } else if normalized.eq_ignore_ascii_case("restrict") {
            Some(Self::Restrict)
        } else if normalized.eq_ignore_ascii_case("cascade") {
            Some(Self::Cascade)
        } else if normalized.eq_ignore_ascii_case("set null") {
            Some(Self::SetNull)
        } else if normalized.eq_ignore_ascii_case("set default") {
            Some(Self::SetDefault)
        } else {
            None
        }
    }
}

/// Column reference endpoint (`[schema.]table.column` or composite `table.(a, b)`) used in a [`Ref`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Endpoint {
    /// Optional schema qualifier (`schema.table`).
    #[serde(default)]
    pub schema: Option<String>,
    /// Referenced table name (canonicalized when resolved via alias).
    pub table: String,
    /// Primary referenced column name (for single-column refs and backwards compatibility).
    pub column: String,
    /// Full list of referenced columns (populated for both single and composite FKs).
    #[serde(default)]
    pub columns: Vec<String>,
}

impl Endpoint {
    /// Returns all referenced column names, falling back to `column` when `columns` is empty.
    #[must_use]
    pub fn cols(&self) -> Vec<&str> {
        if self.columns.is_empty() {
            if self.column.is_empty() {
                Vec::new()
            } else {
                vec![self.column.as_str()]
            }
        } else {
            self.columns.iter().map(String::as_str).collect()
        }
    }

    /// Returns the first referenced column name, or `""` if none.
    #[must_use]
    pub fn primary_column(&self) -> &str {
        self.columns
            .first()
            .map_or(self.column.as_str(), String::as_str)
    }

    /// Returns `schema.table` when qualified, or `table` otherwise.
    #[must_use]
    pub fn qualified_table(&self) -> String {
        format_qualified(self.schema.as_deref(), &self.table)
    }
}

/// Cardinality of a DBML [`Ref`] relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefKind {
    /// Many-to-one (`>`).
    ManyToOne,
    /// One-to-many (`<`).
    OneToMany,
    /// One-to-one (`-`).
    OneToOne,
    /// Many-to-many (`<>`).
    ManyToMany,
}

impl RefKind {
    /// All supported cardinalities in canonical UI order.
    pub const ALL: &[Self] = &[
        Self::ManyToOne,
        Self::OneToMany,
        Self::OneToOne,
        Self::ManyToMany,
    ];

    /// Returns the canonical DBML operator string (`>`, `<`, `-`, `<>`).
    #[must_use]
    pub const fn as_dbml_op(self) -> &'static str {
        match self {
            Self::ManyToOne => ">",
            Self::OneToMany => "<",
            Self::OneToOne => "-",
            Self::ManyToMany => "<>",
        }
    }

    /// Parses a DBML cardinality operator string.
    #[must_use]
    pub fn from_dbml_op(s: &str) -> Option<Self> {
        match s.trim() {
            ">" => Some(Self::ManyToOne),
            "<" => Some(Self::OneToMany),
            "-" => Some(Self::OneToOne),
            "<>" => Some(Self::ManyToMany),
            _ => None,
        }
    }
}

impl Schema {
    /// Returns a reference to the table matching `(schema, name)`.
    ///
    /// When `schema` is `Some(s)`, matches `t.schema.as_deref() == Some(s) && t.name == name`.
    /// When `schema` is `None`, prefers an unqualified table (`t.schema.is_none()`) and falls
    /// back to any table with `t.name == name` when unambiguous.
    #[must_use]
    pub fn table_by(&self, schema: Option<&str>, name: &str) -> Option<&Table> {
        if let Some(s) = schema {
            return self
                .tables
                .iter()
                .find(|t| t.schema.as_deref() == Some(s) && t.name == name);
        }
        self.tables
            .iter()
            .find(|t| t.schema.is_none() && t.name == name)
            .or_else(|| self.tables.iter().find(|t| t.name == name))
    }

    /// Returns a reference to the table with the given `name` (qualified `schema.table` or bare),
    /// if present.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        match name.split_once('.') {
            Some((s, tbl)) => self.table_by(Some(s), tbl),
            None => self.table_by(None, name),
        }
    }

    /// Returns a reference to the [`Group`] containing `table` (qualified or bare), if any.
    #[must_use]
    pub fn group_of(&self, table: &str) -> Option<&Group> {
        let bare = table.split_once('.').map(|(_, n)| n);
        self.groups
            .iter()
            .find(|g| g.tables.iter().any(|t| t == table))
            .or_else(|| {
                let n = bare?;
                self.groups.iter().find(|g| g.tables.iter().any(|t| t == n))
            })
    }

    /// Computes the `depth`-hop FK neighborhood subgraph starting from `seeds` (qualified or bare
    /// table names), traversing both incoming and outgoing relationships (including self-refs).
    #[must_use]
    pub fn neighborhood(&self, seeds: &HashSet<String>, depth: usize) -> FocusSubgraph {
        let resolve = |name: &str| -> String {
            self.table(name)
                .map_or_else(|| name.to_owned(), Table::qualified_name)
        };
        let mut visited: HashSet<String> = seeds.iter().map(|s| resolve(s)).collect();
        let mut frontier = visited.clone();
        let mut ref_indices = HashSet::new();

        for _ in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let mut next_frontier = HashSet::new();
            for (idx, r) in self.refs.iter().enumerate() {
                let from_q = resolve(&r.from.qualified_table());
                let to_q = resolve(&r.to.qualified_table());
                let touches_from = frontier.contains(&from_q);
                let touches_to = frontier.contains(&to_q);
                if touches_from || touches_to {
                    ref_indices.insert(idx);
                    if !visited.contains(&from_q) {
                        next_frontier.insert(from_q);
                    }
                    if !visited.contains(&to_q) {
                        next_frontier.insert(to_q);
                    }
                }
            }
            visited.extend(next_frontier.iter().cloned());
            frontier = next_frontier;
        }

        FocusSubgraph {
            tables: visited,
            ref_indices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schema() -> Schema {
        Schema {
            tables: vec![Table {
                name: "users".to_string(),
                columns: vec![],
                color: None,
                ..Default::default()
            }],
            refs: vec![],
            groups: vec![Group {
                name: "auth".to_string(),
                tables: vec!["users".to_string()],
                color: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn table_should_return_some_when_table_exists() {
        let schema = sample_schema();
        assert_eq!(
            schema.table("users").map(|t| t.name.as_str()),
            Some("users")
        );
    }

    #[test]
    fn table_should_return_none_when_table_missing() {
        let schema = sample_schema();
        assert_eq!(schema.table("orders"), None);
    }

    #[test]
    fn group_of_should_return_group_when_table_is_member() {
        let schema = sample_schema();
        assert_eq!(
            schema.group_of("users").map(|g| g.name.as_str()),
            Some("auth")
        );
    }

    #[test]
    fn group_of_should_return_none_when_table_is_ungrouped() {
        let schema = sample_schema();
        assert_eq!(schema.group_of("orders"), None);
    }

    #[test]
    fn ref_action_roundtrips_dbml_str() {
        for &action in RefAction::ALL {
            assert_eq!(RefAction::from_dbml_str(action.as_dbml_str()), Some(action));
        }
        assert_eq!(
            RefAction::from_dbml_str("SET NULL"),
            Some(RefAction::SetNull)
        );
        assert_eq!(RefAction::from_dbml_str("unknown"), None);
    }

    #[test]
    fn ref_kind_roundtrips_dbml_op() {
        for &kind in RefKind::ALL {
            assert_eq!(RefKind::from_dbml_op(kind.as_dbml_op()), Some(kind));
        }
        assert_eq!(RefKind::from_dbml_op("??"), None);
    }

    #[test]
    fn endpoint_cols_falls_back_to_single_column() {
        let legacy = Endpoint {
            schema: None,
            table: "posts".to_string(),
            column: "user_id".to_string(),
            columns: Vec::new(),
        };
        assert_eq!(legacy.cols(), vec!["user_id"]);
        assert_eq!(legacy.primary_column(), "user_id");

        let composite = Endpoint {
            schema: Some("public".to_string()),
            table: "items".to_string(),
            column: "a".to_string(),
            columns: vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(composite.cols(), vec!["a", "b"]);
        assert_eq!(composite.primary_column(), "a");
    }

    #[test]
    fn index_type_and_nullability_roundtrip() {
        for &ty in IndexType::ALL {
            assert_eq!(IndexType::from_dbml_str(ty.as_sql_str()), Some(ty));
        }
        assert_eq!(IndexType::from_dbml_str("GIN"), Some(IndexType::Gin));
        assert_eq!(IndexType::Gin.as_sql_str(), "gin");
        assert_eq!(IndexType::from_dbml_str("unknown"), None);
        assert!(Nullability::NotNull.is_not_null());
        assert!(!Nullability::Nullable.is_not_null());
        assert!(!Nullability::Unspecified.is_not_null());
    }

    #[test]
    fn index_def_formats_canonical_dbml_line() {
        let single = IndexDef {
            columns: vec![IndexColumn::Column("email".to_string())],
            unique: false,
            pk: false,
            name: None,
            index_type: None,
        };
        assert_eq!(single.to_dbml_line(), "email");
        assert!(single.references_column("email"));
        assert!(!single.references_column("id"));

        let composite = IndexDef {
            columns: vec![
                IndexColumn::Column("tenant_id".to_string()),
                IndexColumn::Column("email".to_string()),
            ],
            unique: true,
            pk: false,
            name: Some("idx_tenant_email".to_string()),
            index_type: Some(IndexType::BTree),
        };
        assert_eq!(
            composite.to_dbml_line(),
            "(tenant_id, email) [unique, type: btree, name: 'idx_tenant_email']"
        );
    }

    #[test]
    fn schema_table_by_distinguishes_same_name_across_schemas() {
        let schema = Schema {
            tables: vec![
                Table {
                    schema: Some("auth".into()),
                    name: "users".into(),
                    ..Default::default()
                },
                Table {
                    schema: Some("billing".into()),
                    name: "users".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            schema.table("auth.users").map(Table::qualified_name),
            Some("auth.users".into())
        );
        assert_eq!(
            schema.table("billing.users").map(Table::qualified_name),
            Some("billing.users".into())
        );
        assert_eq!(schema.table_by(Some("audit"), "users"), None);
    }

    #[test]
    fn neighborhood_computes_1_hop_2_hop_and_self_refs() {
        let ep = |t: &str, c: &str| Endpoint {
            table: t.into(),
            column: c.into(),
            ..Default::default()
        };
        let r = |ft: &str, fc: &str, tt: &str, tc: &str| Ref {
            from: ep(ft, fc),
            to: ep(tt, tc),
            kind: RefKind::ManyToOne,
            on_delete: None,
            on_update: None,
        };
        let schema = Schema {
            tables: ["users", "orders", "order_items", "products", "audit_logs"]
                .into_iter()
                .map(|n| Table {
                    name: n.into(),
                    ..Default::default()
                })
                .collect(),
            refs: vec![
                r("orders", "user_id", "users", "id"), // 0: orders -> users
                r("order_items", "order_id", "orders", "id"), // 1: order_items -> orders
                r("order_items", "product_id", "products", "id"), // 2: order_items -> products
                r("users", "manager_id", "users", "id"), // 3: self-ref on users
            ],
            ..Default::default()
        };

        let seeds = HashSet::from(["users".to_string()]);
        let hop1 = schema.neighborhood(&seeds, 1);
        assert_eq!(
            hop1.tables,
            HashSet::from(["users".to_string(), "orders".to_string()])
        );
        assert_eq!(hop1.ref_indices, HashSet::from([0, 3]));

        let hop2 = schema.neighborhood(&seeds, 2);
        assert_eq!(
            hop2.tables,
            HashSet::from([
                "users".to_string(),
                "orders".to_string(),
                "order_items".to_string()
            ])
        );
        assert_eq!(hop2.ref_indices, HashSet::from([0, 1, 3]));
        assert!(!hop2.tables.contains("audit_logs"));
    }
}
