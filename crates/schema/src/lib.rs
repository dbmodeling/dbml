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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}
