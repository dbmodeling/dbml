//! Diff estrutural puro entre dois [`Schema`] (base vs. head).
//!
//! O casamento é feito estritamente por nome/identidade, sem heurística de rename:
//! renomear uma tabela, coluna, enum ou índice aparece como `Removed` + `Added`.
//! A saída é determinística: itens da base na ordem original, depois itens só do head
//! na ordem original.

use serde::{Deserialize, Serialize};

use crate::{Column, Endpoint, EnumDef, EnumValue, IndexColumn, IndexDef, Ref, Schema, Table};

/// Mudança em um item identificado por chave (coluna, índice, valor de enum, ref).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Change<T> {
    /// Item existe apenas no head.
    Added(T),
    /// Item existe apenas na base.
    Removed(T),
    /// Item existe nos dois lados, mas com conteúdo diferente.
    Modified {
        /// Versão na base.
        before: T,
        /// Versão no head.
        after: T,
    },
}

/// Propriedades de nível de tabela (fora colunas e índices).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableProps {
    /// Cor do cabeçalho (`[headercolor: #hex]`).
    pub color: Option<String>,
    /// Alias DBML (`Table users as u`).
    pub alias: Option<String>,
    /// Nota da tabela.
    pub note: Option<String>,
}

impl TableProps {
    fn of(table: &Table) -> Self {
        Self {
            color: table.color.clone(),
            alias: table.alias.clone(),
            note: table.note.clone(),
        }
    }
}

/// Alteração de propriedades de nível de tabela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TablePropsChange {
    /// Propriedades na base.
    pub before: TableProps,
    /// Propriedades no head.
    pub after: TableProps,
}

/// Detalhe de uma tabela presente nos dois lados e alterada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableModified {
    /// Qualificador de schema da tabela.
    pub schema: Option<String>,
    /// Nome da tabela.
    pub name: String,
    /// Mudança de cor/alias/nota, quando houver.
    pub props: Option<TablePropsChange>,
    /// Mudanças de colunas (casadas por nome).
    pub columns: Vec<Change<Column>>,
    /// Mudanças de índices (casados por nome, ou pela lista de colunas quando sem nome).
    pub indexes: Vec<Change<IndexDef>>,
}

/// Mudança em uma tabela (casada por `schema` + `name`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TableChange {
    /// Tabela existe apenas no head.
    Added(Table),
    /// Tabela existe apenas na base.
    Removed(Table),
    /// Tabela existe nos dois lados com diferenças internas.
    Modified(TableModified),
}

/// Detalhe de um enum presente nos dois lados e alterado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumModified {
    /// Qualificador de schema do enum.
    pub schema: Option<String>,
    /// Nome do enum.
    pub name: String,
    /// Mudanças de valores (casados por nome).
    pub values: Vec<Change<EnumValue>>,
}

/// Mudança em um enum (casado por `schema` + `name`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum EnumChange {
    /// Enum existe apenas no head.
    Added(EnumDef),
    /// Enum existe apenas na base.
    Removed(EnumDef),
    /// Enum existe nos dois lados com valores diferentes.
    Modified(EnumModified),
}

/// Contagem agregada de mudanças atômicas de um [`SchemaDiff`].
///
/// Cada tabela/coluna/índice/enum/valor/ref adicionado, removido ou modificado conta uma vez.
/// Uma tabela `Modified` só conta como modificada se suas próprias propriedades
/// (cor/alias/nota) mudaram; as mudanças de colunas e índices são contadas à parte.
/// O mesmo vale para enums `Modified`, que apenas agrupam mudanças de valores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Itens adicionados.
    pub added: usize,
    /// Itens removidos.
    pub removed: usize,
    /// Itens modificados.
    pub modified: usize,
}

impl DiffSummary {
    /// Total de mudanças.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.added + self.removed + self.modified
    }

    fn count<T>(&mut self, changes: &[Change<T>]) {
        for change in changes {
            match change {
                Change::Added(_) => self.added += 1,
                Change::Removed(_) => self.removed += 1,
                Change::Modified { .. } => self.modified += 1,
            }
        }
    }
}

/// Diferença estrutural entre dois [`Schema`].
///
/// Cobre tabelas (com colunas e índices aninhados), enums (com valores) e refs.
/// Grupos e nota do projeto ficam fora desta fatia.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SchemaDiff {
    /// Mudanças de tabelas.
    pub tables: Vec<TableChange>,
    /// Mudanças de enums.
    pub enums: Vec<EnumChange>,
    /// Mudanças de refs (casadas pelos endpoints `from` e `to`).
    pub refs: Vec<Change<Ref>>,
}

impl SchemaDiff {
    /// `true` quando os dois schemas são estruturalmente equivalentes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.enums.is_empty() && self.refs.is_empty()
    }

    /// Conta mudanças atômicas (ver [`DiffSummary`]).
    #[must_use]
    pub fn summary(&self) -> DiffSummary {
        let mut s = DiffSummary::default();
        for t in &self.tables {
            match t {
                TableChange::Added(_) => s.added += 1,
                TableChange::Removed(_) => s.removed += 1,
                TableChange::Modified(m) => {
                    if m.props.is_some() {
                        s.modified += 1;
                    }
                    s.count(&m.columns);
                    s.count(&m.indexes);
                }
            }
        }
        for e in &self.enums {
            match e {
                EnumChange::Added(_) => s.added += 1,
                EnumChange::Removed(_) => s.removed += 1,
                EnumChange::Modified(m) => s.count(&m.values),
            }
        }
        s.count(&self.refs);
        s
    }
}

/// Resultado do pareamento por chave entre base e head.
enum Pair<'a, T> {
    Both(&'a T, &'a T),
    BaseOnly(&'a T),
    HeadOnly(&'a T),
}

/// Pareia itens por chave. Ordem: itens da base (casados ou removidos), depois os só do head.
/// Chaves duplicadas casam em ordem de aparição (1º com 1º, 2º com 2º).
fn pair_by_key<'a, T, K: Eq>(
    base: &'a [T],
    head: &'a [T],
    key: impl Fn(&T) -> K,
) -> Vec<Pair<'a, T>> {
    let base_keys: Vec<K> = base.iter().map(&key).collect();
    let head_keys: Vec<K> = head.iter().map(&key).collect();
    let mut head_used = vec![false; head.len()];
    let mut out = Vec::with_capacity(base.len().max(head.len()));

    for (b, bk) in base.iter().zip(&base_keys) {
        let found = head_keys
            .iter()
            .enumerate()
            .find(|(i, hk)| !head_used[*i] && *hk == bk);
        match found {
            Some((i, _)) => {
                head_used[i] = true;
                if let Some(h) = head.get(i) {
                    out.push(Pair::Both(b, h));
                }
            }
            None => out.push(Pair::BaseOnly(b)),
        }
    }
    for (h, used) in head.iter().zip(&head_used) {
        if !used {
            out.push(Pair::HeadOnly(h));
        }
    }
    out
}

fn diff_items<T: Clone, K: Eq>(
    base: &[T],
    head: &[T],
    key: impl Fn(&T) -> K,
    differs: impl Fn(&T, &T) -> bool,
) -> Vec<Change<T>> {
    pair_by_key(base, head, key)
        .into_iter()
        .filter_map(|pair| match pair {
            Pair::BaseOnly(b) => Some(Change::Removed(b.clone())),
            Pair::HeadOnly(h) => Some(Change::Added(h.clone())),
            Pair::Both(b, h) if differs(b, h) => Some(Change::Modified {
                before: b.clone(),
                after: h.clone(),
            }),
            Pair::Both(..) => None,
        })
        .collect()
}

/// `fk` é projeção derivada das refs (já reportadas em `refs`), então não conta como
/// modificação de coluna por si só; os demais campos contam.
fn column_differs(base: &Column, head: &Column) -> bool {
    let mut normalized = base.clone();
    normalized.fk = head.fk;
    normalized != *head
}

/// Identidade de índice: nome explícito (não vazio) ou, na falta, a lista de colunas/expressões.
fn index_key(index: &IndexDef) -> String {
    if let Some(name) = index
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        return format!("name:{name}");
    }
    let cols: Vec<String> = index.columns.iter().map(IndexColumn::as_dbml_str).collect();
    format!("cols:{}", cols.join(","))
}

type EndpointKey = (Option<String>, String, Vec<String>);

fn endpoint_key(e: &Endpoint) -> EndpointKey {
    (
        e.schema.clone(),
        e.table.clone(),
        e.cols().into_iter().map(str::to_owned).collect(),
    )
}

fn ref_key(r: &Ref) -> (EndpointKey, EndpointKey) {
    (endpoint_key(&r.from), endpoint_key(&r.to))
}

/// Endpoints já fazem parte da chave (comparados por `cols()`, imune à forma legada de
/// `Endpoint`); só cardinalidade e ações referenciais contam como modificação.
fn ref_differs(base: &Ref, head: &Ref) -> bool {
    base.kind != head.kind || base.on_delete != head.on_delete || base.on_update != head.on_update
}

fn diff_table(base: &Table, head: &Table) -> Option<TableModified> {
    let before = TableProps::of(base);
    let after = TableProps::of(head);
    let props = (before != after).then_some(TablePropsChange { before, after });
    let columns = diff_items(
        &base.columns,
        &head.columns,
        |c| c.name.clone(),
        column_differs,
    );
    let indexes = diff_items(&base.indexes, &head.indexes, index_key, |a, b| a != b);
    if props.is_none() && columns.is_empty() && indexes.is_empty() {
        return None;
    }
    Some(TableModified {
        schema: head.schema.clone(),
        name: head.name.clone(),
        props,
        columns,
        indexes,
    })
}

fn diff_enum(base: &EnumDef, head: &EnumDef) -> Option<EnumModified> {
    let values = diff_items(
        &base.values,
        &head.values,
        |v| v.name.clone(),
        |a, b| a != b,
    );
    if values.is_empty() {
        return None;
    }
    Some(EnumModified {
        schema: head.schema.clone(),
        name: head.name.clone(),
        values,
    })
}

/// Calcula o diff estrutural de `base` para `head`.
///
/// Casamento estritamente por nome: tabelas e enums por `(schema, name)`, colunas e valores
/// de enum por nome, índices por nome (ou colunas, se anônimos) e refs pelos endpoints.
/// Não há detecção de rename (vira `Removed` + `Added`).
#[must_use]
pub fn diff_schemas(base: &Schema, head: &Schema) -> SchemaDiff {
    let tables = pair_by_key(&base.tables, &head.tables, |t| {
        (t.schema.clone(), t.name.clone())
    })
    .into_iter()
    .filter_map(|pair| match pair {
        Pair::BaseOnly(b) => Some(TableChange::Removed(b.clone())),
        Pair::HeadOnly(h) => Some(TableChange::Added(h.clone())),
        Pair::Both(b, h) => diff_table(b, h).map(TableChange::Modified),
    })
    .collect();

    let enums = pair_by_key(&base.enums, &head.enums, |e| {
        (e.schema.clone(), e.name.clone())
    })
    .into_iter()
    .filter_map(|pair| match pair {
        Pair::BaseOnly(b) => Some(EnumChange::Removed(b.clone())),
        Pair::HeadOnly(h) => Some(EnumChange::Added(h.clone())),
        Pair::Both(b, h) => diff_enum(b, h).map(EnumChange::Modified),
    })
    .collect();

    let refs = diff_items(&base.refs, &head.refs, ref_key, ref_differs);

    SchemaDiff {
        tables,
        enums,
        refs,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::{IndexType, Nullability, RefAction, RefKind};

    fn col(name: &str, ty: &str) -> Column {
        Column {
            name: name.into(),
            ty: ty.into(),
            ..Default::default()
        }
    }

    fn table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            name: name.into(),
            columns,
            ..Default::default()
        }
    }

    fn schema_with(tables: Vec<Table>) -> Schema {
        Schema {
            tables,
            ..Default::default()
        }
    }

    fn endpoint(table: &str, column: &str) -> Endpoint {
        Endpoint {
            schema: None,
            table: table.into(),
            column: column.into(),
            columns: vec![column.into()],
        }
    }

    fn fk_ref(from: (&str, &str), to: (&str, &str)) -> Ref {
        Ref {
            from: endpoint(from.0, from.1),
            to: endpoint(to.0, to.1),
            kind: RefKind::ManyToOne,
            on_delete: None,
            on_update: None,
        }
    }

    fn index(cols: &[&str], name: Option<&str>) -> IndexDef {
        IndexDef {
            columns: cols
                .iter()
                .map(|c| IndexColumn::Column((*c).into()))
                .collect(),
            name: name.map(Into::into),
            ..Default::default()
        }
    }

    fn enum_def(name: &str, values: &[&str]) -> EnumDef {
        EnumDef {
            schema: None,
            name: name.into(),
            values: values
                .iter()
                .map(|v| EnumValue {
                    name: (*v).into(),
                    note: None,
                })
                .collect(),
        }
    }

    type ColumnMutation = Box<dyn Fn(&mut Column)>;

    fn modified_table(diff: &SchemaDiff) -> &TableModified {
        match diff.tables.as_slice() {
            [TableChange::Modified(m)] => m,
            other => panic!("esperava uma tabela Modified, veio {other:?}"),
        }
    }

    // ---------- identidade / vazio ----------

    #[test]
    fn identical_schemas_yield_empty_diff() {
        let mut s = schema_with(vec![table("users", vec![col("id", "int")])]);
        s.refs.push(fk_ref(("posts", "user_id"), ("users", "id")));
        s.enums.push(enum_def("role", &["admin", "user"]));
        let diff = diff_schemas(&s, &s.clone());
        assert!(diff.is_empty());
        assert_eq!(diff.summary(), DiffSummary::default());
        assert_eq!(diff.summary().total(), 0);
    }

    #[test]
    fn empty_schemas_yield_empty_diff() {
        assert!(diff_schemas(&Schema::default(), &Schema::default()).is_empty());
    }

    // ---------- tabelas ----------

    #[test]
    fn table_added() {
        let base = Schema::default();
        let head = schema_with(vec![table("users", vec![col("id", "int")])]);
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            diff.tables,
            vec![TableChange::Added(table("users", vec![col("id", "int")]))]
        );
        assert!(!diff.is_empty());
        assert_eq!(
            diff.summary(),
            DiffSummary {
                added: 1,
                removed: 0,
                modified: 0
            }
        );
    }

    #[test]
    fn table_removed() {
        let base = schema_with(vec![table("users", vec![])]);
        let diff = diff_schemas(&base, &Schema::default());
        assert_eq!(
            diff.tables,
            vec![TableChange::Removed(table("users", vec![]))]
        );
        assert_eq!(diff.summary().removed, 1);
    }

    #[test]
    fn table_props_modified() {
        let base = schema_with(vec![table("users", vec![])]);
        let mut head_table = table("users", vec![]);
        head_table.note = Some("contas".into());
        head_table.color = Some("#ff0000".into());
        let diff = diff_schemas(&base, &schema_with(vec![head_table]));
        let m = modified_table(&diff);
        let props = m.props.as_ref().expect("props mudou");
        assert_eq!(props.before, TableProps::default());
        assert_eq!(props.after.note.as_deref(), Some("contas"));
        assert_eq!(props.after.color.as_deref(), Some("#ff0000"));
        assert!(m.columns.is_empty() && m.indexes.is_empty());
        assert_eq!(diff.summary().modified, 1);
    }

    #[test]
    fn same_table_name_in_different_schemas_are_distinct() {
        let mut a = table("users", vec![]);
        a.schema = Some("auth".into());
        let mut b = table("users", vec![]);
        b.schema = Some("billing".into());
        let diff = diff_schemas(&schema_with(vec![a.clone()]), &schema_with(vec![b.clone()]));
        assert_eq!(
            diff.tables,
            vec![TableChange::Removed(a), TableChange::Added(b)]
        );
    }

    #[test]
    fn table_rename_is_removed_plus_added() {
        let base = schema_with(vec![table("users", vec![col("id", "int")])]);
        let head = schema_with(vec![table("accounts", vec![col("id", "int")])]);
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            diff.tables,
            vec![
                TableChange::Removed(table("users", vec![col("id", "int")])),
                TableChange::Added(table("accounts", vec![col("id", "int")])),
            ]
        );
        assert_eq!(diff.summary().added, 1);
        assert_eq!(diff.summary().removed, 1);
        assert_eq!(diff.summary().modified, 0);
    }

    // ---------- colunas ----------

    #[test]
    fn column_added_and_removed() {
        let base = schema_with(vec![table(
            "users",
            vec![col("id", "int"), col("old", "text")],
        )]);
        let head = schema_with(vec![table(
            "users",
            vec![col("id", "int"), col("email", "text")],
        )]);
        let diff = diff_schemas(&base, &head);
        let m = modified_table(&diff);
        assert!(m.props.is_none());
        assert_eq!(
            m.columns,
            vec![
                Change::Removed(col("old", "text")),
                Change::Added(col("email", "text")),
            ]
        );
        assert_eq!(diff.summary().added, 1);
        assert_eq!(diff.summary().removed, 1);
        assert_eq!(diff.summary().modified, 0);
    }

    #[test]
    fn column_type_modified() {
        let base = schema_with(vec![table("users", vec![col("age", "int")])]);
        let head = schema_with(vec![table("users", vec![col("age", "bigint")])]);
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            modified_table(&diff).columns,
            vec![Change::Modified {
                before: col("age", "int"),
                after: col("age", "bigint"),
            }]
        );
        assert_eq!(diff.summary().modified, 1);
    }

    #[test]
    fn column_flags_each_trigger_modified() {
        let variants: Vec<ColumnMutation> = vec![
            Box::new(|c| c.pk = true),
            Box::new(|c| c.not_null = true),
            Box::new(|c| c.unique = true),
            Box::new(|c| c.increment = true),
            Box::new(|c| c.default_value = Some("0".into())),
            Box::new(|c| c.nullability = Nullability::Nullable),
            Box::new(|c| c.note = Some("n".into())),
            Box::new(|c| c.check = Some("x > 0".into())),
        ];
        for mutate in variants {
            let before = col("x", "int");
            let mut after = before.clone();
            mutate(&mut after);
            let diff = diff_schemas(
                &schema_with(vec![table("t", vec![before.clone()])]),
                &schema_with(vec![table("t", vec![after.clone()])]),
            );
            assert_eq!(
                modified_table(&diff).columns,
                vec![Change::Modified { before, after }]
            );
        }
    }

    #[test]
    fn column_fk_flag_alone_is_not_a_modification() {
        let before = col("user_id", "int");
        let mut after = before.clone();
        after.fk = true;
        let diff = diff_schemas(
            &schema_with(vec![table("posts", vec![before])]),
            &schema_with(vec![table("posts", vec![after])]),
        );
        assert!(diff.is_empty());
    }

    #[test]
    fn column_rename_is_removed_plus_added() {
        let base = schema_with(vec![table("t", vec![col("a", "int")])]);
        let head = schema_with(vec![table("t", vec![col("b", "int")])]);
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            modified_table(&diff).columns,
            vec![
                Change::Removed(col("a", "int")),
                Change::Added(col("b", "int"))
            ]
        );
    }

    #[test]
    fn column_reorder_alone_is_not_a_change() {
        let base = schema_with(vec![table("t", vec![col("a", "int"), col("b", "int")])]);
        let head = schema_with(vec![table("t", vec![col("b", "int"), col("a", "int")])]);
        assert!(diff_schemas(&base, &head).is_empty());
    }

    // ---------- índices ----------

    #[test]
    fn index_added_removed_modified() {
        let mut base_t = table("users", vec![]);
        base_t.indexes = vec![
            index(&["email"], Some("idx_email")),
            index(&["a"], Some("idx_gone")),
        ];
        let mut head_t = table("users", vec![]);
        let mut changed = index(&["email"], Some("idx_email"));
        changed.unique = true;
        changed.index_type = Some(IndexType::Hash);
        head_t.indexes = vec![changed.clone(), index(&["b"], Some("idx_new"))];

        let diff = diff_schemas(&schema_with(vec![base_t]), &schema_with(vec![head_t]));
        assert_eq!(
            modified_table(&diff).indexes,
            vec![
                Change::Modified {
                    before: index(&["email"], Some("idx_email")),
                    after: changed,
                },
                Change::Removed(index(&["a"], Some("idx_gone"))),
                Change::Added(index(&["b"], Some("idx_new"))),
            ]
        );
        assert_eq!(diff.summary().added, 1);
        assert_eq!(diff.summary().removed, 1);
        assert_eq!(diff.summary().modified, 1);
    }

    #[test]
    fn anonymous_index_matches_by_columns() {
        let mut base_t = table("t", vec![]);
        base_t.indexes = vec![index(&["a", "b"], None)];
        let mut head_t = table("t", vec![]);
        let mut changed = index(&["a", "b"], None);
        changed.unique = true;
        head_t.indexes = vec![changed.clone()];
        let diff = diff_schemas(&schema_with(vec![base_t]), &schema_with(vec![head_t]));
        assert_eq!(
            modified_table(&diff).indexes,
            vec![Change::Modified {
                before: index(&["a", "b"], None),
                after: changed
            }]
        );
    }

    #[test]
    fn index_rename_is_removed_plus_added() {
        let mut base_t = table("t", vec![]);
        base_t.indexes = vec![index(&["a"], Some("old"))];
        let mut head_t = table("t", vec![]);
        head_t.indexes = vec![index(&["a"], Some("new"))];
        let diff = diff_schemas(&schema_with(vec![base_t]), &schema_with(vec![head_t]));
        assert_eq!(
            modified_table(&diff).indexes,
            vec![
                Change::Removed(index(&["a"], Some("old"))),
                Change::Added(index(&["a"], Some("new"))),
            ]
        );
    }

    // ---------- enums ----------

    #[test]
    fn enum_added_and_removed() {
        let base = Schema {
            enums: vec![enum_def("gone", &["x"])],
            ..Default::default()
        };
        let head = Schema {
            enums: vec![enum_def("role", &["admin"])],
            ..Default::default()
        };
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            diff.enums,
            vec![
                EnumChange::Removed(enum_def("gone", &["x"])),
                EnumChange::Added(enum_def("role", &["admin"])),
            ]
        );
        assert!(!diff.is_empty());
        assert_eq!(diff.summary().added, 1);
        assert_eq!(diff.summary().removed, 1);
    }

    #[test]
    fn enum_values_added_removed_modified() {
        let mut base_enum = enum_def("role", &["admin", "guest", "user"]);
        base_enum.values[2].note = Some("padrao".into());
        let mut head_enum = enum_def("role", &["admin", "user", "owner"]);
        head_enum.values[1].note = Some("comum".into());
        let base = Schema {
            enums: vec![base_enum],
            ..Default::default()
        };
        let head = Schema {
            enums: vec![head_enum],
            ..Default::default()
        };
        let diff = diff_schemas(&base, &head);
        let [EnumChange::Modified(m)] = diff.enums.as_slice() else {
            panic!("esperava enum Modified, veio {:?}", diff.enums);
        };
        assert_eq!(m.name, "role");
        assert_eq!(m.values.len(), 3);
        assert!(matches!(&m.values[0], Change::Removed(v) if v.name == "guest"));
        assert!(matches!(
            &m.values[1],
            Change::Modified { before, after }
                if before.name == "user"
                    && before.note.as_deref() == Some("padrao")
                    && after.note.as_deref() == Some("comum")
        ));
        assert!(matches!(&m.values[2], Change::Added(v) if v.name == "owner"));
        assert_eq!(
            diff.summary(),
            DiffSummary {
                added: 1,
                removed: 1,
                modified: 1
            }
        );
    }

    #[test]
    fn enum_rename_is_removed_plus_added() {
        let base = Schema {
            enums: vec![enum_def("role", &["a"])],
            ..Default::default()
        };
        let head = Schema {
            enums: vec![enum_def("kind", &["a"])],
            ..Default::default()
        };
        let diff = diff_schemas(&base, &head);
        assert_eq!(diff.enums.len(), 2);
        assert!(matches!(&diff.enums[0], EnumChange::Removed(e) if e.name == "role"));
        assert!(matches!(&diff.enums[1], EnumChange::Added(e) if e.name == "kind"));
    }

    // ---------- refs ----------

    #[test]
    fn ref_added_and_removed() {
        let r_old = fk_ref(("posts", "author_id"), ("users", "id"));
        let r_new = fk_ref(("posts", "user_id"), ("users", "id"));
        let base = Schema {
            refs: vec![r_old.clone()],
            ..Default::default()
        };
        let head = Schema {
            refs: vec![r_new.clone()],
            ..Default::default()
        };
        let diff = diff_schemas(&base, &head);
        assert_eq!(
            diff.refs,
            vec![Change::Removed(r_old), Change::Added(r_new)]
        );
        assert_eq!(diff.summary().added, 1);
        assert_eq!(diff.summary().removed, 1);
    }

    #[test]
    fn ref_kind_and_actions_modified() {
        let before = fk_ref(("posts", "user_id"), ("users", "id"));
        let mut after = before.clone();
        after.kind = RefKind::OneToOne;
        after.on_delete = Some(RefAction::Cascade);
        after.on_update = Some(RefAction::Restrict);
        let diff = diff_schemas(
            &Schema {
                refs: vec![before.clone()],
                ..Default::default()
            },
            &Schema {
                refs: vec![after.clone()],
                ..Default::default()
            },
        );
        assert_eq!(diff.refs, vec![Change::Modified { before, after }]);
        assert_eq!(diff.summary().modified, 1);
    }

    #[test]
    fn ref_matches_by_endpoint_columns_not_legacy_column_field() {
        // `columns` vazio + `column` preenchido equivale a `columns = [column]`.
        let legacy = Ref {
            from: Endpoint {
                schema: None,
                table: "posts".into(),
                column: "user_id".into(),
                columns: Vec::new(),
            },
            ..fk_ref(("posts", "user_id"), ("users", "id"))
        };
        let modern = fk_ref(("posts", "user_id"), ("users", "id"));
        let diff = diff_schemas(
            &Schema {
                refs: vec![legacy],
                ..Default::default()
            },
            &Schema {
                refs: vec![modern],
                ..Default::default()
            },
        );
        assert!(diff.is_empty());
    }

    // ---------- determinismo / serde ----------

    #[test]
    fn order_is_base_first_then_head_only() {
        let base = schema_with(vec![table("b", vec![]), table("a", vec![])]);
        let head = schema_with(vec![
            table("z", vec![]),
            table("a", vec![]),
            table("y", vec![]),
        ]);
        let diff = diff_schemas(&base, &head);
        let names: Vec<String> = diff
            .tables
            .iter()
            .map(|t| match t {
                TableChange::Added(t) | TableChange::Removed(t) => t.name.clone(),
                TableChange::Modified(m) => m.name.clone(),
            })
            .collect();
        assert_eq!(names, vec!["b", "z", "y"]);
    }

    #[test]
    fn diff_is_deterministic_and_serializes_stably() {
        let base = schema_with(vec![
            table("users", vec![col("id", "int"), col("age", "int")]),
            table("old", vec![]),
        ]);
        let mut head = schema_with(vec![
            table("users", vec![col("id", "int"), col("age", "bigint")]),
            table("new", vec![]),
        ]);
        head.refs.push(fk_ref(("new", "u"), ("users", "id")));
        let first = diff_schemas(&base, &head);
        let second = diff_schemas(&base, &head);
        assert_eq!(first, second);
        let a = serde_json::to_string(&first).expect("serializa");
        let b = serde_json::to_string(&second).expect("serializa");
        assert_eq!(a, b);
        assert!(a.contains("\"kind\":\"modified\""));
        assert!(a.contains("\"kind\":\"added\""));
        assert!(a.contains("\"kind\":\"removed\""));
    }

    #[test]
    fn diff_roundtrips_through_json() {
        let base = schema_with(vec![table("a", vec![col("x", "int")])]);
        let head = schema_with(vec![table("a", vec![col("x", "text")])]);
        let diff = diff_schemas(&base, &head);
        let json = serde_json::to_string(&diff).expect("serializa");
        let back: SchemaDiff = serde_json::from_str(&json).expect("desserializa");
        assert_eq!(diff, back);
    }
}
