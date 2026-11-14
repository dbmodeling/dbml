//! Resolve pass: binds late constraints, indexes and comments to their tables and drops anything
//! that points at an unknown table or column, so the emitter can only produce valid references.

use crate::SqlDialect;
use crate::model::{
    Column, EnumType, Fk, FkAction, ImportWarning, IndexCol, Ir, Name, TableConstraint, Text,
};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexKind {
    Plain,
    Unique,
    PrimaryKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RIndex<'a> {
    pub(crate) name: Option<Text<'a>>,
    pub(crate) cols: Vec<IndexCol<'a>>,
    pub(crate) kind: IndexKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RTable<'a> {
    pub(crate) name: Name<'a>,
    pub(crate) columns: Vec<Column<'a>>,
    pub(crate) indexes: Vec<RIndex<'a>>,
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RRef<'a> {
    pub(crate) from_table: Name<'a>,
    pub(crate) from_cols: Vec<Text<'a>>,
    pub(crate) to_table: Name<'a>,
    pub(crate) to_cols: Vec<Text<'a>>,
    pub(crate) one_to_one: bool,
    pub(crate) on_delete: Option<FkAction>,
    pub(crate) on_update: Option<FkAction>,
}

/// An [`Ir`] whose references are all valid; the only input accepted by the emitter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved<'a> {
    pub(crate) tables: Vec<RTable<'a>>,
    pub(crate) enums: Vec<EnumType<'a>>,
    pub(crate) refs: Vec<RRef<'a>>,
    pub(crate) warnings: Vec<ImportWarning>,
}

type Pos = (u32, u32);

fn warn(warnings: &mut Vec<ImportWarning>, (line, column): Pos, message: String) {
    warnings.push(ImportWarning {
        line,
        column,
        message,
    });
}

fn same_name(a: &Name<'_>, b: &Name<'_>) -> bool {
    a.name.eq_ignore_ascii_case(&b.name)
        && match (&a.schema, &b.schema) {
            (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
            (None, None) => true,
            _ => false,
        }
}

/// Finds the table `target` refers to: same schema first, then any table with that name.
fn find_table(tables: &[RTable<'_>], target: &Name<'_>) -> Option<usize> {
    tables
        .iter()
        .position(|t| same_name(&t.name, target))
        .or_else(|| {
            tables
                .iter()
                .position(|t| t.name.name.eq_ignore_ascii_case(&target.name))
        })
}

fn strip_public(name: &mut Name<'_>, default_schema: &str) {
    if name
        .schema
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case(default_schema))
    {
        name.schema = None;
    }
}

fn strip_public_type(ty: &mut String) {
    const PREFIX: &str = "public.";
    if ty.len() >= PREFIX.len()
        && ty
            .get(..PREFIX.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(PREFIX))
    {
        ty.drain(..PREFIX.len());
    }
}

/// The default schema (`public` on PostgreSQL, `dbo` on SQL Server) is noise in DBML: drop the
/// qualifier everywhere.
fn strip_public_schema(ir: &mut Ir<'_>, default_schema: &str) {
    fn fk(f: &mut Fk<'_>, d: &str) {
        strip_public(&mut f.to_table, d);
    }
    fn constraint(k: &mut TableConstraint<'_>, d: &str) {
        if let TableConstraint::ForeignKey(f) = k {
            fk(f, d);
        }
    }
    for t in &mut ir.tables {
        strip_public(&mut t.name, default_schema);
        t.constraints
            .iter_mut()
            .for_each(|k| constraint(k, default_schema));
        for c in &mut t.columns {
            strip_public_type(&mut c.ty);
            c.references.iter_mut().for_each(|f| fk(f, default_schema));
        }
    }
    ir.enums
        .iter_mut()
        .for_each(|e| strip_public(&mut e.name, default_schema));
    ir.indexes
        .iter_mut()
        .for_each(|i| strip_public(&mut i.table, default_schema));
    ir.comments
        .iter_mut()
        .for_each(|c| strip_public(&mut c.table, default_schema));
    for a in &mut ir.alter_constraints {
        strip_public(&mut a.table, default_schema);
        constraint(&mut a.constraint, default_schema);
    }
}

/// Maps `cols` to the table's own spelling; `None` (with a warning) when one does not exist.
fn known_cols<'a>(
    table: &RTable<'a>,
    cols: &[Text<'a>],
    what: &str,
    pos: Pos,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Vec<Text<'a>>> {
    let mut out = Vec::with_capacity(cols.len());
    for c in cols {
        let Some(actual) = table
            .columns
            .iter()
            .find(|k| k.name.eq_ignore_ascii_case(c))
        else {
            warn(
                warnings,
                pos,
                format!(
                    "{what} on `{}` references unknown column `{c}`; dropped",
                    table.name.name
                ),
            );
            return None;
        };
        out.push(actual.name.clone());
    }
    (!out.is_empty()).then_some(out)
}

fn apply_constraint<'a>(
    tables: &mut [RTable<'a>],
    idx: usize,
    constraint: TableConstraint<'a>,
    pending: &mut Vec<(usize, Fk<'a>)>,
    warnings: &mut Vec<ImportWarning>,
    pos: Pos,
) {
    let Some(table) = tables.get_mut(idx) else {
        return;
    };
    match constraint {
        TableConstraint::ForeignKey(fk) => pending.push((idx, fk)),
        TableConstraint::PrimaryKey(cols) => {
            let Some(cols) = known_cols(table, &cols, "primary key", pos, warnings) else {
                return;
            };
            fold_key(table, cols, None, IndexKind::PrimaryKey);
        }
        TableConstraint::Unique { name, cols } => {
            let Some(cols) = known_cols(table, &cols, "unique constraint", pos, warnings) else {
                return;
            };
            fold_key(table, cols, name, IndexKind::Unique);
        }
    }
}

/// A single-column key becomes a column setting; a composite key becomes an `indexes` entry.
fn fold_key<'a>(
    table: &mut RTable<'a>,
    cols: Vec<Text<'a>>,
    name: Option<Text<'a>>,
    kind: IndexKind,
) {
    if let [only] = cols.as_slice() {
        if let Some(c) = table.columns.iter_mut().find(|c| c.name == *only) {
            match kind {
                IndexKind::PrimaryKey => c.primary_key = true,
                _ => c.unique = true,
            }
        }
    } else {
        table.indexes.push(RIndex {
            name,
            cols: cols.into_iter().map(IndexCol::Column).collect(),
            kind,
        });
    }
}

/// Turns inline enums (MySQL `ENUM(..)`, SQLite `CHECK (.. IN ..)`) into named enums.
fn synthesize_enums<'a>(tables: &mut [RTable<'a>], enums: &mut Vec<EnumType<'a>>) {
    for table in tables {
        for col in &mut table.columns {
            let Some(values) = col.enum_values.take() else {
                continue;
            };
            let base = format!("{}_{}", table.name.name, col.name);
            let mut name = base.clone();
            let mut n = 2;
            while enums
                .iter()
                .any(|e| e.name.schema == table.name.schema && e.name.name == name)
            {
                name = format!("{base}_{n}");
                n += 1;
            }
            col.ty = match &table.name.schema {
                Some(s) => format!("{s}.{name}"),
                None => name.clone(),
            };
            enums.push(EnumType {
                name: Name {
                    schema: table.name.schema.clone(),
                    name: Cow::Owned(name),
                },
                values,
            });
        }
    }
}

fn sorted_eq(a: &[Text<'_>], b: &[IndexCol<'_>]) -> bool {
    let mut x: Vec<&str> = a.iter().map(AsRef::as_ref).collect();
    let mut y: Vec<&str> = b
        .iter()
        .filter_map(|c| match c {
            IndexCol::Column(c) => Some(c.as_ref()),
            IndexCol::Expr(_) => None,
        })
        .collect();
    x.sort_unstable();
    y.sort_unstable();
    x == y
}

fn target_key_cols<'a>(target: &RTable<'a>) -> Vec<Text<'a>> {
    let single: Vec<Text<'a>> = target
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.clone())
        .collect();
    if !single.is_empty() {
        return single;
    }
    target
        .indexes
        .iter()
        .find(|i| i.kind == IndexKind::PrimaryKey)
        .map(|i| {
            i.cols
                .iter()
                .filter_map(|c| match c {
                    IndexCol::Column(c) => Some(c.clone()),
                    IndexCol::Expr(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_fk<'a>(
    tables: &[RTable<'a>],
    idx: usize,
    fk: Fk<'a>,
    warnings: &mut Vec<ImportWarning>,
) -> Option<RRef<'a>> {
    let pos = (fk.line, fk.col);
    let from = tables.get(idx)?;
    let Some(to) = find_table(tables, &fk.to_table).and_then(|i| tables.get(i)) else {
        warn(
            warnings,
            pos,
            format!(
                "foreign key on `{}` references unknown table `{}`; dropped",
                from.name.name, fk.to_table.name
            ),
        );
        return None;
    };
    let from_cols = known_cols(from, &fk.from_cols, "foreign key", pos, warnings)?;
    let to_cols = if fk.to_cols.is_empty() {
        target_key_cols(to)
    } else {
        fk.to_cols
    };
    if to_cols.is_empty() {
        warn(
            warnings,
            pos,
            format!(
                "foreign key from `{}` to `{}` omits the target columns and the target has no primary key; dropped",
                from.name.name, to.name.name
            ),
        );
        return None;
    }
    let to_cols = known_cols(to, &to_cols, "foreign key target", pos, warnings)?;
    if to_cols.len() != from_cols.len() {
        warn(
            warnings,
            pos,
            format!(
                "foreign key from `{}` to `{}` has mismatched column counts; dropped",
                from.name.name, to.name.name
            ),
        );
        return None;
    }
    let one_to_one = match from_cols.as_slice() {
        [only] => from
            .columns
            .iter()
            .any(|c| c.name == *only && (c.primary_key || c.unique)),
        _ => from.indexes.iter().any(|i| {
            matches!(i.kind, IndexKind::PrimaryKey | IndexKind::Unique)
                && sorted_eq(&from_cols, &i.cols)
        }),
    };
    Some(RRef {
        from_table: from.name.clone(),
        from_cols,
        to_table: to.name.clone(),
        to_cols,
        one_to_one,
        on_delete: fk.on_delete,
        on_update: fk.on_update,
    })
}

/// Binds late constraints, indexes and comments, then validates every reference.
pub(crate) fn resolve<'a>(mut ir: Ir<'a>, dialect: SqlDialect) -> Resolved<'a> {
    match dialect {
        SqlDialect::Postgres => strip_public_schema(&mut ir, "public"),
        SqlDialect::SqlServer => strip_public_schema(&mut ir, "dbo"),
        _ => {}
    }
    let Ir {
        tables,
        mut enums,
        indexes,
        alter_constraints,
        comments,
        mut warnings,
    } = ir;

    let mut out: Vec<RTable<'a>> = Vec::with_capacity(tables.len());
    let mut pending: Vec<(usize, Fk<'a>)> = Vec::new();
    for t in tables {
        let pos = (t.line, t.col);
        if out.iter().any(|o| same_name(&o.name, &t.name)) {
            warn(
                &mut warnings,
                pos,
                format!(
                    "table `{}` is defined more than once; the first definition is kept",
                    t.name.name
                ),
            );
            continue;
        }
        let idx = out.len();
        let mut table = RTable {
            name: t.name,
            columns: t.columns,
            indexes: Vec::new(),
            comment: t.comment,
        };
        for col in &mut table.columns {
            if let Some(fk) = col.references.take() {
                pending.push((idx, fk));
            }
        }
        out.push(table);
        for k in t.constraints {
            apply_constraint(&mut out, idx, k, &mut pending, &mut warnings, pos);
        }
    }

    for a in alter_constraints {
        let pos = (a.line, a.col);
        match find_table(&out, &a.table) {
            Some(idx) => apply_constraint(
                &mut out,
                idx,
                a.constraint,
                &mut pending,
                &mut warnings,
                pos,
            ),
            None => warn(
                &mut warnings,
                pos,
                format!(
                    "ALTER TABLE references unknown table `{}`; dropped",
                    a.table.name
                ),
            ),
        }
    }

    for ix in indexes {
        let pos = (ix.line, ix.col);
        let Some(idx) = find_table(&out, &ix.table) else {
            warn(
                &mut warnings,
                pos,
                format!("index on unknown table `{}` dropped", ix.table.name),
            );
            continue;
        };
        let Some(table) = out.get_mut(idx) else {
            continue;
        };
        let mut cols = Vec::with_capacity(ix.cols.len());
        let mut valid = true;
        for c in ix.cols {
            match c {
                IndexCol::Column(name) => {
                    match known_cols(
                        table,
                        std::slice::from_ref(&name),
                        "index",
                        pos,
                        &mut warnings,
                    )
                    .and_then(|mut v| v.pop())
                    {
                        Some(actual) => cols.push(IndexCol::Column(actual)),
                        None => valid = false,
                    }
                }
                expr @ IndexCol::Expr(_) => cols.push(expr),
            }
        }
        if valid && !cols.is_empty() {
            table.indexes.push(RIndex {
                name: ix.name,
                cols,
                kind: if ix.unique {
                    IndexKind::Unique
                } else {
                    IndexKind::Plain
                },
            });
        }
    }

    for c in comments {
        let Some(table) = find_table(&out, &c.table).and_then(|i| out.get_mut(i)) else {
            continue;
        };
        match c.column {
            None => table.comment = Some(c.text),
            Some(col) => {
                if let Some(target) = table
                    .columns
                    .iter_mut()
                    .find(|k| k.name.eq_ignore_ascii_case(&col))
                {
                    target.comment = Some(c.text);
                }
            }
        }
    }

    synthesize_enums(&mut out, &mut enums);

    let refs = pending
        .into_iter()
        .filter_map(|(idx, fk)| resolve_fk(&out, idx, fk, &mut warnings))
        .collect();

    Resolved {
        tables: out,
        enums,
        refs,
        warnings,
    }
}
