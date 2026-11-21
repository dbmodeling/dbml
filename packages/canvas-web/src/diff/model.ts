/**
 * Modelo de visualização do modo Comparar: transforma o `DiffResult` do Wasm (`diff_dbml`) em
 * estruturas prontas para desenhar (status por tabela/coluna/enum/relação) e em uma lista plana
 * de itens para o painel de resumo. Módulo puro: sem DOM, sem Wasm, sem i18n (os textos de
 * detalhe só usam símbolos e palavras-chave DBML, iguais em qualquer idioma).
 */

import {
  edgeKey,
  qualifiedEndpointTable,
  qualifiedTableName,
  type Column,
  type DiffChange,
  type DiffResult,
  type DiffSummary,
  type DiffTableProps,
  type EnumDef,
  type EnumValueDef,
  type IndexDef,
  type Ref,
  type RefKind,
  type Table,
} from '../types';

export type DiffStatus = 'added' | 'removed' | 'modified';
export type DiffGroup = 'tables' | 'columns' | 'indexes' | 'enums' | 'refs';

/** Ordem de exibição dos grupos no painel. */
export const DIFF_GROUPS: readonly DiffGroup[] = ['tables', 'columns', 'indexes', 'enums', 'refs'];

/** Marcador textual por status (acessibilidade: nunca só cor). */
export const STATUS_MARK: Record<DiffStatus, string> = { added: '+', modified: '~', removed: '−' };

export type ColumnField = 'ty' | 'pk' | 'not_null' | 'unique' | 'default_value' | 'check' | 'note';

export interface FieldChange {
  field: ColumnField;
  before: string | boolean | null;
  after: string | boolean | null;
}

export interface ColumnDiff {
  name: string;
  status: DiffStatus;
  column: Column;
  /** Só em `modified`. */
  before?: Column;
  changes: FieldChange[];
}

export interface TableDiff {
  qname: string;
  status: 'added' | 'modified';
  columns: Map<string, ColumnDiff>;
  /** Colunas removidas: desenhadas no fim do cartão (não deslocam as linhas das arestas). */
  ghostColumns: Column[];
}

export interface EnumDiff {
  name: string;
  status: 'added' | 'modified';
  values: Map<string, DiffStatus>;
  ghostValues: string[];
}

export interface RefDiff {
  key: string;
  status: DiffStatus;
  ref: Ref;
  before?: Ref;
}

export type DiffTarget =
  | { kind: 'table'; name: string; removed: boolean }
  | { kind: 'enum'; name: string; removed: boolean }
  | { kind: 'ref'; key: string; removed: boolean };

export interface DiffItem {
  id: string;
  group: DiffGroup;
  status: DiffStatus;
  /** Identificadores técnicos (`tabela.coluna`, `a → b`): não traduzidos. */
  label: string;
  /** Descrição curta da alteração (tipo, flags, cardinalidade); vazia quando não há. */
  detail: string;
  target: DiffTarget;
}

export interface DiffView {
  summary: DiffSummary;
  tables: Map<string, TableDiff>;
  removedTables: Table[];
  enums: Map<string, EnumDiff>;
  removedEnums: EnumDef[];
  /** Relações adicionadas, removidas ou modificadas, por chave de aresta (`edgeKey`). */
  refs: Map<string, RefDiff>;
  items: DiffItem[];
}

const KIND_LABEL: Record<RefKind, string> = {
  ManyToOne: 'N:1',
  OneToMany: '1:N',
  OneToOne: '1:1',
  ManyToMany: 'N:N',
};

export const refKindLabel = (kind: RefKind): string => KIND_LABEL[kind];

/** Chave da relação, igual à `edgeKey` das arestas roteadas. */
export function refKey(ref: Ref): string {
  return edgeKey({
    from_table: qualifiedEndpointTable(ref.from),
    from_col: ref.from.column,
    to_table: qualifiedEndpointTable(ref.to),
    to_col: ref.to.column,
  });
}

export function refLabel(ref: Ref): string {
  return `${qualifiedEndpointTable(ref.from)}.${ref.from.column} → ${qualifiedEndpointTable(ref.to)}.${ref.to.column}`;
}

const FIELDS: ReadonlyArray<readonly [ColumnField, (c: Column) => string | boolean | null]> = [
  ['ty', (c) => c.ty],
  ['pk', (c) => c.pk],
  ['not_null', (c) => c.not_null],
  ['unique', (c) => c.unique],
  ['default_value', (c) => c.default_value ?? null],
  ['check', (c) => c.check ?? null],
  ['note', (c) => c.note ?? null],
];

/** Campos que mudaram entre duas versões da mesma coluna (`fk` é derivado das refs e fica de fora). */
export function columnFieldChanges(before: Column, after: Column): FieldChange[] {
  const out: FieldChange[] = [];
  for (const [field, read] of FIELDS) {
    const b = read(before);
    const a = read(after);
    if (b !== a) out.push({ field, before: b, after: a });
  }
  return out;
}

const FLAG_WORD: Partial<Record<ColumnField, string>> = { pk: 'pk', not_null: 'not null', unique: 'unique' };

const show = (v: string | boolean | null): string => (v === null || v === '' ? '∅' : String(v));

/** `int → bigint`, `+not null`, `−unique`, `default 1 → 2`; vocabulário DBML, igual em qualquer idioma. */
export function formatFieldChange(change: FieldChange): string {
  const word = FLAG_WORD[change.field];
  if (word) return `${change.after ? '+' : '−'}${word}`;
  if (change.field === 'ty') return `${show(change.before)} → ${show(change.after)}`;
  if (change.field === 'note') return 'note';
  const name = change.field === 'default_value' ? 'default' : change.field;
  return `${name} ${show(change.before)} → ${show(change.after)}`;
}

export function indexLabel(ix: IndexDef): string {
  const name = ix.name?.trim();
  if (name) return name;
  return `(${ix.columns.map((c) => c.value).join(', ')})`;
}

function tablePropsDetail(before: DiffTableProps, after: DiffTableProps): string {
  const parts: string[] = [];
  if ((before.color ?? null) !== (after.color ?? null)) parts.push(`color ${show(before.color ?? null)} → ${show(after.color ?? null)}`);
  if ((before.alias ?? null) !== (after.alias ?? null)) parts.push(`alias ${show(before.alias ?? null)} → ${show(after.alias ?? null)}`);
  if ((before.note ?? null) !== (after.note ?? null)) parts.push('note');
  return parts.join(', ');
}

function refDetail(before: Ref, after: Ref): string {
  const parts: string[] = [];
  if (before.kind !== after.kind) parts.push(`${KIND_LABEL[before.kind]} → ${KIND_LABEL[after.kind]}`);
  if ((before.on_delete ?? null) !== (after.on_delete ?? null)) {
    parts.push(`on delete ${show(before.on_delete ?? null)} → ${show(after.on_delete ?? null)}`);
  }
  if ((before.on_update ?? null) !== (after.on_update ?? null)) {
    parts.push(`on update ${show(before.on_update ?? null)} → ${show(after.on_update ?? null)}`);
  }
  return parts.join(', ');
}

/** Monta o modelo de visualização do diff (as listas já vêm ordenadas pelo núcleo). */
export function buildDiffView(result: DiffResult): DiffView {
  const tables = new Map<string, TableDiff>();
  const removedTables: Table[] = [];
  const enums = new Map<string, EnumDiff>();
  const removedEnums: EnumDef[] = [];
  const refs = new Map<string, RefDiff>();
  const byGroup: Record<DiffGroup, DiffItem[]> = { tables: [], columns: [], indexes: [], enums: [], refs: [] };

  const push = (item: Omit<DiffItem, 'id'>): void => {
    byGroup[item.group].push({ ...item, id: `${item.group}:${item.status}:${item.label}` });
  };

  for (const change of result.diff.tables) {
    if (change.kind === 'added') {
      const qname = qualifiedTableName(change.value);
      tables.set(qname, { qname, status: 'added', columns: new Map(), ghostColumns: [] });
      push({ group: 'tables', status: 'added', label: qname, detail: `${change.value.columns.length}`, target: { kind: 'table', name: qname, removed: false } });
    } else if (change.kind === 'removed') {
      const qname = qualifiedTableName(change.value);
      removedTables.push(change.value);
      push({ group: 'tables', status: 'removed', label: qname, detail: `${change.value.columns.length}`, target: { kind: 'table', name: qname, removed: true } });
    } else {
      const m = change.value;
      const qname = qualifiedTableName(m);
      const view: TableDiff = { qname, status: 'modified', columns: new Map(), ghostColumns: [] };
      tables.set(qname, view);
      const target: DiffTarget = { kind: 'table', name: qname, removed: false };
      if (m.props) {
        push({ group: 'tables', status: 'modified', label: qname, detail: tablePropsDetail(m.props.before, m.props.after), target });
      }
      for (const c of m.columns) {
        if (c.kind === 'added') {
          view.columns.set(c.value.name, { name: c.value.name, status: 'added', column: c.value, changes: [] });
          push({ group: 'columns', status: 'added', label: `${qname}.${c.value.name}`, detail: c.value.ty, target });
        } else if (c.kind === 'removed') {
          view.ghostColumns.push(c.value);
          view.columns.set(c.value.name, { name: c.value.name, status: 'removed', column: c.value, changes: [] });
          push({ group: 'columns', status: 'removed', label: `${qname}.${c.value.name}`, detail: c.value.ty, target });
        } else {
          const { before, after } = c.value;
          const changes = columnFieldChanges(before, after);
          view.columns.set(after.name, { name: after.name, status: 'modified', column: after, before, changes });
          push({ group: 'columns', status: 'modified', label: `${qname}.${after.name}`, detail: changes.map(formatFieldChange).join(', '), target });
        }
      }
      for (const ix of m.indexes) pushIndex(push, qname, ix, target);
    }
  }

  for (const change of result.diff.enums) {
    if (change.kind === 'added') {
      enums.set(change.value.name, { name: change.value.name, status: 'added', values: new Map(), ghostValues: [] });
      push({ group: 'enums', status: 'added', label: change.value.name, detail: `${change.value.values.length}`, target: { kind: 'enum', name: change.value.name, removed: false } });
    } else if (change.kind === 'removed') {
      removedEnums.push(change.value);
      push({ group: 'enums', status: 'removed', label: change.value.name, detail: `${change.value.values.length}`, target: { kind: 'enum', name: change.value.name, removed: true } });
    } else {
      const m = change.value;
      const view: EnumDiff = { name: m.name, status: 'modified', values: new Map(), ghostValues: [] };
      enums.set(m.name, view);
      const target: DiffTarget = { kind: 'enum', name: m.name, removed: false };
      for (const v of m.values) enumValue(view, v, push, target);
    }
  }

  for (const change of result.diff.refs) {
    const ref = change.kind === 'modified' ? change.value.after : change.value;
    const key = refKey(ref);
    const before = change.kind === 'modified' ? change.value.before : undefined;
    refs.set(key, { key, status: change.kind, ref, before });
    push({
      group: 'refs',
      status: change.kind,
      label: refLabel(ref),
      detail: before ? refDetail(before, ref) : refKindLabel(ref.kind),
      target: { kind: 'ref', key, removed: change.kind === 'removed' },
    });
  }

  return {
    summary: result.summary,
    tables,
    removedTables,
    enums,
    removedEnums,
    refs,
    items: DIFF_GROUPS.flatMap((g) => byGroup[g]),
  };
}

function pushIndex(
  push: (item: Omit<DiffItem, 'id'>) => void,
  qname: string,
  change: DiffChange<IndexDef>,
  target: DiffTarget
): void {
  const ix = change.kind === 'modified' ? change.value.after : change.value;
  const flags = [ix.unique ? 'unique' : '', ix.pk ? 'pk' : ''].filter(Boolean).join(' ');
  push({ group: 'indexes', status: change.kind, label: `${qname} · ${indexLabel(ix)}`, detail: flags, target });
}

function enumValue(
  view: EnumDiff,
  change: DiffChange<EnumValueDef>,
  push: (item: Omit<DiffItem, 'id'>) => void,
  target: DiffTarget
): void {
  const v = change.kind === 'modified' ? change.value.after : change.value;
  view.values.set(v.name, change.kind);
  if (change.kind === 'removed') view.ghostValues.push(v.name);
  push({ group: 'enums', status: change.kind, label: `${view.name}.${v.name}`, detail: '', target });
}

/** Contagem de itens por grupo (cabeçalhos do painel). */
export function groupCounts(view: DiffView): Record<DiffGroup, number> {
  const counts: Record<DiffGroup, number> = { tables: 0, columns: 0, indexes: 0, enums: 0, refs: 0 };
  for (const item of view.items) counts[item.group]++;
  return counts;
}

export function isEmptyDiff(view: DiffView): boolean {
  return view.items.length === 0;
}
