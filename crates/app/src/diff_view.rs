//! Modelo de visualização do modo Comparar (#18): transforma um [`schema::SchemaDiff`] em
//! estruturas prontas para desenhar (status por tabela/coluna/enum/relação), em uma lista plana de
//! itens para o painel de resumo e na geometria dos cartões fantasma (removidos).
//!
//! Módulo puro (sem GPUI, sem i18n): espelha `packages/canvas-web/src/diff/{model,layout}.ts`.
//! Os textos de detalhe só usam símbolos e palavras-chave DBML, iguais em qualquer idioma.

use std::collections::HashMap;

use schema::{
    Change, Column, DiffSummary, EnumChange, EnumDef, EnumValue, IndexColumn, IndexDef, Ref,
    RefAction, RefKind, SchemaDiff, Table, TableChange, TablePropsChange,
};

/// Status de um item no diff (base → head).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
}

impl DiffStatus {
    /// Marcador textual (acessibilidade: o status nunca é só cor).
    pub const fn mark(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Modified => "~",
            Self::Removed => "−",
        }
    }

    fn of<T>(change: &Change<T>) -> Self {
        match change {
            Change::Added(_) => Self::Added,
            Change::Removed(_) => Self::Removed,
            Change::Modified { .. } => Self::Modified,
        }
    }
}

/// Grupo do painel de resumo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffGroup {
    Tables,
    Columns,
    Indexes,
    Enums,
    Refs,
}

impl DiffGroup {
    /// Ordem de exibição dos grupos no painel.
    pub const ALL: [Self; 5] = [
        Self::Tables,
        Self::Columns,
        Self::Indexes,
        Self::Enums,
        Self::Refs,
    ];
}

/// Para onde o clique em um item do resumo leva a vista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTarget {
    Table { name: String, removed: bool },
    Enum { name: String, removed: bool },
    Ref { key: String, removed: bool },
}

/// Linha do painel de resumo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffItem {
    pub group: DiffGroup,
    pub status: DiffStatus,
    /// Identificadores técnicos (`tabela.coluna`, `a.x → b.y`): não traduzidos.
    pub label: String,
    /// Descrição curta da alteração (tipo, flags, cardinalidade); vazia quando não há.
    pub detail: String,
    pub target: DiffTarget,
}

/// Tabela presente no head com mudanças (adicionada ou modificada).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDiff {
    pub status: DiffStatus,
    /// Status por nome de coluna (inclui as removidas).
    pub columns: HashMap<String, DiffStatus>,
    /// Colunas removidas: desenhadas no fim do cartão (não deslocam as linhas das arestas).
    pub ghost_columns: Vec<Column>,
}

/// Enum presente no head com mudanças (adicionado ou modificado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDiff {
    pub status: DiffStatus,
    pub values: HashMap<String, DiffStatus>,
    pub ghost_values: Vec<String>,
}

/// Relação adicionada, removida ou modificada (`r` é a versão do head, ou da base se removida).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefDiff {
    pub status: DiffStatus,
    pub r: Ref,
}

/// Modelo completo do modo Comparar.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffView {
    pub summary: DiffSummary,
    /// Por nome qualificado da tabela no head.
    pub tables: HashMap<String, TableDiff>,
    pub removed_tables: Vec<Table>,
    /// Por nome do enum no head.
    pub enums: HashMap<String, EnumDiff>,
    pub removed_enums: Vec<EnumDef>,
    /// Por [`ref_key`].
    pub refs: HashMap<String, RefDiff>,
    /// Itens do resumo, agrupados na ordem de [`DiffGroup::ALL`].
    pub items: Vec<DiffItem>,
}

impl DiffView {
    /// `true` quando base e head são estruturalmente iguais.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Quantidade de itens de um grupo (cabeçalhos do painel).
    pub fn group_count(&self, group: DiffGroup) -> usize {
        self.items.iter().filter(|i| i.group == group).count()
    }

    /// Status de uma tabela do head (`None` = inalterada).
    pub fn table_status(&self, qname: &str) -> Option<DiffStatus> {
        self.tables.get(qname).map(|t| t.status)
    }

    /// Status de uma coluna do head (`None` = inalterada).
    pub fn column_status(&self, qname: &str, column: &str) -> Option<DiffStatus> {
        self.tables.get(qname)?.columns.get(column).copied()
    }

    /// Status de uma relação do head (`None` = inalterada).
    pub fn ref_status(&self, r: &Ref) -> Option<DiffStatus> {
        self.refs.get(&ref_key(r)).map(|d| d.status)
    }
}

/// Placeholder de valor ausente nos detalhes (`∅`).
const EMPTY_VALUE: &str = "∅";

fn show(value: Option<&str>) -> &str {
    value.filter(|v| !v.is_empty()).unwrap_or(EMPTY_VALUE)
}

fn show_action(value: Option<RefAction>) -> &'static str {
    value.map_or(EMPTY_VALUE, RefAction::as_dbml_str)
}

/// Rótulo de cardinalidade (`N:1`, `1:N`, `1:1`, `N:N`).
pub const fn ref_kind_label(kind: RefKind) -> &'static str {
    match kind {
        RefKind::ManyToOne => "N:1",
        RefKind::OneToMany => "1:N",
        RefKind::OneToOne => "1:1",
        RefKind::ManyToMany => "N:N",
    }
}

/// Chave estável de uma relação (endpoints qualificados + colunas), igual nos dois lados do diff.
pub fn ref_key(r: &Ref) -> String {
    format!(
        "{}.({})>{}.({})",
        r.from.qualified_table(),
        r.from.cols().join(","),
        r.to.qualified_table(),
        r.to.cols().join(",")
    )
}

/// `a.x → b.y` (identificadores técnicos).
pub fn ref_label(r: &Ref) -> String {
    format!(
        "{}.{} → {}.{}",
        r.from.qualified_table(),
        r.from.primary_column(),
        r.to.qualified_table(),
        r.to.primary_column()
    )
}

/// Nome do índice, ou a lista de colunas entre parênteses quando anônimo.
pub fn index_label(ix: &IndexDef) -> String {
    if let Some(name) = ix.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        return name.to_owned();
    }
    let cols: Vec<String> = ix.columns.iter().map(IndexColumn::as_dbml_str).collect();
    format!("({})", cols.join(", "))
}

/// `int → bigint, +not null, −unique, default 1 → 2, note` (vocabulário DBML; `fk` fica de fora,
/// pois é derivado das relações).
pub fn column_change_detail(before: &Column, after: &Column) -> String {
    let mut parts: Vec<String> = Vec::new();
    if before.ty != after.ty {
        parts.push(format!(
            "{} → {}",
            show(Some(&before.ty)),
            show(Some(&after.ty))
        ));
    }
    for (word, b, a) in [
        ("pk", before.pk, after.pk),
        ("not null", before.not_null, after.not_null),
        ("unique", before.unique, after.unique),
    ] {
        if b != a {
            let sign = if a { "+" } else { "−" };
            parts.push(format!("{sign}{word}"));
        }
    }
    if before.default_value != after.default_value {
        parts.push(format!(
            "default {} → {}",
            show(before.default_value.as_deref()),
            show(after.default_value.as_deref())
        ));
    }
    if before.check != after.check {
        parts.push(format!(
            "check {} → {}",
            show(before.check.as_deref()),
            show(after.check.as_deref())
        ));
    }
    if before.note != after.note {
        parts.push("note".to_owned());
    }
    parts.join(", ")
}

fn table_props_detail(change: &TablePropsChange) -> String {
    let (b, a) = (&change.before, &change.after);
    let mut parts: Vec<String> = Vec::new();
    if b.color != a.color {
        parts.push(format!(
            "color {} → {}",
            show(b.color.as_deref()),
            show(a.color.as_deref())
        ));
    }
    if b.alias != a.alias {
        parts.push(format!(
            "alias {} → {}",
            show(b.alias.as_deref()),
            show(a.alias.as_deref())
        ));
    }
    if b.note != a.note {
        parts.push("note".to_owned());
    }
    parts.join(", ")
}

fn ref_detail(before: &Ref, after: &Ref) -> String {
    let mut parts: Vec<String> = Vec::new();
    if before.kind != after.kind {
        parts.push(format!(
            "{} → {}",
            ref_kind_label(before.kind),
            ref_kind_label(after.kind)
        ));
    }
    if before.on_delete != after.on_delete {
        parts.push(format!(
            "on delete {} → {}",
            show_action(before.on_delete),
            show_action(after.on_delete)
        ));
    }
    if before.on_update != after.on_update {
        parts.push(format!(
            "on update {} → {}",
            show_action(before.on_update),
            show_action(after.on_update)
        ));
    }
    parts.join(", ")
}

fn changed<T>(change: &Change<T>) -> &T {
    match change {
        Change::Added(v) | Change::Removed(v) => v,
        Change::Modified { after, .. } => after,
    }
}

/// Monta o modelo de visualização do diff (as listas já vêm ordenadas pelo núcleo).
pub fn build_diff_view(diff: &SchemaDiff) -> DiffView {
    let mut view = DiffView {
        summary: diff.summary(),
        ..DiffView::default()
    };
    let mut by_group: HashMap<DiffGroup, Vec<DiffItem>> = HashMap::new();
    let mut push = |group, status, label: String, detail: String, target| {
        by_group.entry(group).or_default().push(DiffItem {
            group,
            status,
            label,
            detail,
            target,
        });
    };

    for change in &diff.tables {
        match change {
            TableChange::Added(t) | TableChange::Removed(t) => {
                let qname = t.qualified_name();
                let removed = matches!(change, TableChange::Removed(_));
                let status = if removed {
                    DiffStatus::Removed
                } else {
                    DiffStatus::Added
                };
                if removed {
                    view.removed_tables.push(t.clone());
                } else {
                    view.tables.insert(
                        qname.clone(),
                        TableDiff {
                            status,
                            columns: HashMap::new(),
                            ghost_columns: Vec::new(),
                        },
                    );
                }
                push(
                    DiffGroup::Tables,
                    status,
                    qname.clone(),
                    t.columns.len().to_string(),
                    DiffTarget::Table {
                        name: qname,
                        removed,
                    },
                );
            }
            TableChange::Modified(m) => {
                let qname = Table {
                    schema: m.schema.clone(),
                    name: m.name.clone(),
                    ..Table::default()
                }
                .qualified_name();
                let target = DiffTarget::Table {
                    name: qname.clone(),
                    removed: false,
                };
                let mut table = TableDiff {
                    status: DiffStatus::Modified,
                    columns: HashMap::new(),
                    ghost_columns: Vec::new(),
                };
                if let Some(props) = &m.props {
                    push(
                        DiffGroup::Tables,
                        DiffStatus::Modified,
                        qname.clone(),
                        table_props_detail(props),
                        target.clone(),
                    );
                }
                for c in &m.columns {
                    let status = DiffStatus::of(c);
                    let col = changed(c);
                    let detail = match c {
                        Change::Modified { before, after } => column_change_detail(before, after),
                        _ => col.ty.clone(),
                    };
                    if status == DiffStatus::Removed {
                        table.ghost_columns.push(col.clone());
                    }
                    table.columns.insert(col.name.clone(), status);
                    push(
                        DiffGroup::Columns,
                        status,
                        format!("{qname}.{}", col.name),
                        detail,
                        target.clone(),
                    );
                }
                for ix in &m.indexes {
                    let def = changed(ix);
                    let flags: Vec<&str> = [(def.unique, "unique"), (def.pk, "pk")]
                        .into_iter()
                        .filter_map(|(on, w)| on.then_some(w))
                        .collect();
                    push(
                        DiffGroup::Indexes,
                        DiffStatus::of(ix),
                        format!("{qname} · {}", index_label(def)),
                        flags.join(" "),
                        target.clone(),
                    );
                }
                view.tables.insert(qname, table);
            }
        }
    }

    for change in &diff.enums {
        match change {
            EnumChange::Added(e) | EnumChange::Removed(e) => {
                let removed = matches!(change, EnumChange::Removed(_));
                let status = if removed {
                    DiffStatus::Removed
                } else {
                    DiffStatus::Added
                };
                if removed {
                    view.removed_enums.push(e.clone());
                } else {
                    view.enums.insert(
                        e.name.clone(),
                        EnumDiff {
                            status,
                            values: HashMap::new(),
                            ghost_values: Vec::new(),
                        },
                    );
                }
                push(
                    DiffGroup::Enums,
                    status,
                    e.name.clone(),
                    e.values.len().to_string(),
                    DiffTarget::Enum {
                        name: e.name.clone(),
                        removed,
                    },
                );
            }
            EnumChange::Modified(m) => {
                let mut en = EnumDiff {
                    status: DiffStatus::Modified,
                    values: HashMap::new(),
                    ghost_values: Vec::new(),
                };
                for v in &m.values {
                    let status = DiffStatus::of(v);
                    let value: &EnumValue = changed(v);
                    en.values.insert(value.name.clone(), status);
                    if status == DiffStatus::Removed {
                        en.ghost_values.push(value.name.clone());
                    }
                    push(
                        DiffGroup::Enums,
                        status,
                        format!("{}.{}", m.name, value.name),
                        String::new(),
                        DiffTarget::Enum {
                            name: m.name.clone(),
                            removed: false,
                        },
                    );
                }
                view.enums.insert(m.name.clone(), en);
            }
        }
    }

    for change in &diff.refs {
        let status = DiffStatus::of(change);
        let r = changed(change);
        let key = ref_key(r);
        let detail = match change {
            Change::Modified { before, after } => ref_detail(before, after),
            _ => ref_kind_label(r.kind).to_owned(),
        };
        push(
            DiffGroup::Refs,
            status,
            ref_label(r),
            detail,
            DiffTarget::Ref {
                key: key.clone(),
                removed: status == DiffStatus::Removed,
            },
        );
        view.refs.insert(
            key,
            RefDiff {
                status,
                r: r.clone(),
            },
        );
    }

    view.items = DiffGroup::ALL
        .iter()
        .flat_map(|g| by_group.remove(g).unwrap_or_default())
        .collect();
    view
}

/// Posiciona os cartões fantasma (`(largura, altura)` por slot, na ordem) sem sobrepor
/// `occupied`: fileiras abaixo do diagrama, alinhadas à esquerda do conteúdo existente, com no
/// máximo `row_max_w` de largura. Retorna a posição de cada slot na mesma ordem.
pub fn place_ghosts(
    occupied: &[layout::Rect],
    slots: &[(f32, f32)],
    gap: f32,
    row_max_w: f32,
) -> Vec<(f32, f32)> {
    let left = occupied.iter().map(|r| r.x).reduce(f32::min).unwrap_or(0.);
    let bottom = occupied.iter().map(|r| r.y + r.h).reduce(f32::max);
    let (mut x, mut y) = (left, bottom.map_or(0., |b| b + 2. * gap));
    let mut row_h: f32 = 0.;
    let mut out = Vec::with_capacity(slots.len());
    for &(w, h) in slots {
        if x > left && x + w > left + row_max_w {
            x = left;
            y += row_h + gap;
            row_h = 0.;
        }
        out.push((x, y));
        x += w + gap;
        row_h = row_h.max(h);
    }
    out
}

/// Índice de linha da coluna no cartão desenhado: colunas do head primeiro, depois as removidas
/// (fantasma) e, para tabelas removidas, as colunas da base. `None` se a coluna não existe.
pub fn column_row(qname: &str, column: &str, head: &[Table], view: &DiffView) -> Option<usize> {
    if let Some(t) = head.iter().find(|t| t.qualified_name() == qname) {
        if let Some(i) = t.columns.iter().position(|c| c.name == column) {
            return Some(i);
        }
        let ghost = view.tables.get(qname)?;
        let g = ghost.ghost_columns.iter().position(|c| c.name == column)?;
        return Some(t.columns.len() + g);
    }
    view.removed_tables
        .iter()
        .find(|t| t.qualified_name() == qname)?
        .columns
        .iter()
        .position(|c| c.name == column)
}

/// Quantas linhas o cartão da tabela do head tem no modo Comparar (colunas + fantasmas).
pub fn diff_row_count(t: &Table, view: &DiffView) -> usize {
    t.columns.len()
        + view
            .tables
            .get(&t.qualified_name())
            .map_or(0, |d| d.ghost_columns.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::diff_schemas;

    fn parse(src: &str) -> schema::Schema {
        dbml_analysis::analyze(src)
    }

    fn view(base: &str, head: &str) -> DiffView {
        build_diff_view(&diff_schemas(&parse(base), &parse(head)))
    }

    const BASE: &str = "Table users {\n  id int [pk]\n  email varchar\n  legacy text\n}\n\
        Table posts {\n  id int [pk]\n  user_id int\n}\n\
        Table old_logs {\n  id int\n  msg text\n}\n\
        Enum status {\n  active\n  gone\n}\n\
        Ref: posts.user_id > users.id\n";

    const HEAD: &str = "Table users {\n  id bigint [pk]\n  email varchar [not null]\n  name text\n}\n\
        Table posts {\n  id int [pk]\n  user_id int\n}\n\
        Table tags {\n  id int\n}\n\
        Enum status {\n  active\n  archived\n}\n\
        Ref: posts.user_id - users.id\n";

    #[test]
    fn identical_schemas_produce_empty_view() {
        let v = view(BASE, BASE);
        assert!(v.is_empty());
        assert_eq!(v.summary.total(), 0);
    }

    #[test]
    fn table_statuses_and_ghost_columns() {
        let v = view(BASE, HEAD);
        assert_eq!(v.table_status("tags"), Some(DiffStatus::Added));
        assert_eq!(v.table_status("users"), Some(DiffStatus::Modified));
        assert_eq!(v.table_status("posts"), None);
        assert_eq!(v.removed_tables.len(), 1);
        assert_eq!(v.removed_tables[0].name, "old_logs");
        assert_eq!(v.column_status("users", "name"), Some(DiffStatus::Added));
        assert_eq!(v.column_status("users", "id"), Some(DiffStatus::Modified));
        assert_eq!(
            v.column_status("users", "legacy"),
            Some(DiffStatus::Removed)
        );
        let ghosts: Vec<_> = v.tables["users"]
            .ghost_columns
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(ghosts, ["legacy"]);
    }

    #[test]
    fn items_are_grouped_in_canonical_order_with_details() {
        let v = view(BASE, HEAD);
        let groups: Vec<DiffGroup> = v.items.iter().map(|i| i.group).collect();
        let mut sorted = groups.clone();
        sorted.sort_by_key(|g| DiffGroup::ALL.iter().position(|x| x == g));
        assert_eq!(groups, sorted, "itens fora da ordem dos grupos");
        let find = |label: &str| v.items.iter().find(|i| i.label == label).unwrap();
        assert_eq!(find("users.id").detail, "int → bigint");
        assert_eq!(find("users.email").detail, "+not null");
        assert_eq!(find("users.legacy").detail, "text");
        assert_eq!(find("old_logs").status, DiffStatus::Removed);
        assert_eq!(find("old_logs").detail, "2");
        assert_eq!(find("status.gone").status, DiffStatus::Removed);
        assert_eq!(find("status.archived").status, DiffStatus::Added);
        let r = find("posts.user_id → users.id");
        assert_eq!(r.status, DiffStatus::Modified);
        assert_eq!(r.detail, "N:1 → 1:1");
        // id e email modificadas, legacy removida, name adicionada
        assert_eq!(v.group_count(DiffGroup::Columns), 4);
        assert_eq!(v.group_count(DiffGroup::Tables), 2);
    }

    #[test]
    fn enum_ghost_values_and_ref_status() {
        let v = view(BASE, HEAD);
        assert_eq!(v.enums["status"].status, DiffStatus::Modified);
        assert_eq!(v.enums["status"].ghost_values, ["gone"]);
        let head = parse(HEAD);
        assert_eq!(v.ref_status(&head.refs[0]), Some(DiffStatus::Modified));
    }

    #[test]
    fn removed_ref_targets_ghost_key() {
        let base = "Table a {\n  id int\n}\nTable b {\n  a_id int\n}\nRef: b.a_id > a.id\n";
        let head = "Table a {\n  id int\n}\nTable b {\n  a_id int\n}\n";
        let v = view(base, head);
        let item = &v.items[0];
        assert_eq!(item.group, DiffGroup::Refs);
        assert_eq!(item.status, DiffStatus::Removed);
        assert_eq!(item.detail, "N:1");
        let DiffTarget::Ref { key, removed } = &item.target else {
            panic!("alvo deveria ser relação");
        };
        assert!(*removed);
        assert_eq!(v.refs[key].status, DiffStatus::Removed);
    }

    #[test]
    fn column_detail_formats_flags_default_and_note() {
        let before = Column {
            name: "x".into(),
            ty: "int".into(),
            unique: true,
            default_value: Some("1".into()),
            ..Column::default()
        };
        let after = Column {
            name: "x".into(),
            ty: "int".into(),
            pk: true,
            default_value: None,
            note: Some("n".into()),
            ..Column::default()
        };
        assert_eq!(
            column_change_detail(&before, &after),
            "+pk, −unique, default 1 → ∅, note"
        );
    }

    #[test]
    fn column_row_covers_head_ghost_and_removed_tables() {
        let v = view(BASE, HEAD);
        let head = parse(HEAD);
        assert_eq!(column_row("users", "email", &head.tables, &v), Some(1));
        // `legacy` foi removida: fica depois das 3 colunas do head
        assert_eq!(column_row("users", "legacy", &head.tables, &v), Some(3));
        assert_eq!(column_row("old_logs", "msg", &head.tables, &v), Some(1));
        assert_eq!(column_row("users", "nope", &head.tables, &v), None);
        let users = head.table("users").unwrap();
        assert_eq!(diff_row_count(users, &v), 4);
    }

    #[test]
    fn ghosts_go_below_content_and_wrap_rows() {
        let occupied = [
            layout::Rect {
                x: 100.,
                y: 0.,
                w: 50.,
                h: 100.,
            },
            layout::Rect {
                x: 300.,
                y: 50.,
                w: 50.,
                h: 200.,
            },
        ];
        let out = place_ghosts(&occupied, &[(80., 40.), (80., 60.), (80., 10.)], 10., 180.);
        // abaixo do conteúdo (250) + 2 * gap, alinhado à esquerda (100)
        assert_eq!(out[0], (100., 270.));
        assert_eq!(out[1], (190., 270.));
        // terceira não cabe na fileira de 180: quebra abaixo da mais alta (60) + gap
        assert_eq!(out[2], (100., 340.));
        assert_eq!(place_ghosts(&[], &[(10., 10.)], 5., 100.), [(0., 0.)]);
    }

    #[test]
    fn index_label_uses_name_or_columns() {
        let v = view(
            "Table t {\n  a int\n  b int\n}\n",
            "Table t {\n  a int\n  b int\n  indexes {\n    (a, b) [unique]\n  }\n}\n",
        );
        let item = v
            .items
            .iter()
            .find(|i| i.group == DiffGroup::Indexes)
            .unwrap();
        assert_eq!(item.label, "t · (a, b)");
        assert_eq!(item.detail, "unique");
        assert_eq!(item.status, DiffStatus::Added);
    }
}
