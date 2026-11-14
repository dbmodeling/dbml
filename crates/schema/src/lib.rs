#![forbid(unsafe_code)]
#![deny(missing_docs)]
//! Pure semantic database schema model (tables, columns, relationships, and groups),
//! derived as a projection from the lossless DBML CST.

use serde::{Deserialize, Serialize};

/// Semantic representation of a DBML schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Schema {
    /// Tables defined in the schema.
    pub tables: Vec<Table>,
    /// Foreign-key relationships between columns.
    pub refs: Vec<Ref>,
    /// Named `TableGroup` containers grouping related tables.
    pub groups: Vec<Group>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    /// Table name.
    pub name: String,
    /// Ordered list of columns in this table.
    pub columns: Vec<Column>,
    /// Optional hex header color (`[headercolor: #hex]`).
    pub color: Option<String>,
}

/// A column inside a [`Table`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Column reference endpoint (`table.column`) used in a [`Ref`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Referenced table name.
    pub table: String,
    /// Referenced column name.
    pub column: String,
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
    /// Returns a reference to the table with the given `name`, if present.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// Returns a reference to the [`Group`] containing `table`, if any.
    #[must_use]
    pub fn group_of(&self, table: &str) -> Option<&Group> {
        self.groups
            .iter()
            .find(|g| g.tables.iter().any(|t| t == table))
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
            }],
            refs: vec![],
            groups: vec![Group {
                name: "auth".to_string(),
                tables: vec!["users".to_string()],
                color: None,
            }],
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
}
