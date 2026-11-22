/**
 * HTML do modo Comparar: painel de resumo (sidebar), cartões fantasma de itens removidos e
 * arestas removidas. Funções puras (string → string); o Workbench só as injeta no DOM.
 * Cores: verde = adicionado, âmbar = modificado, vermelho = removido (variáveis do tema), e todo
 * item leva também um marcador de texto (`+`, `~`, `−`) para não depender só de cor.
 */

import {
  DIFF_ACT,
  DIFF_GHOST_DASH,
  DIFF_GHOST_EDGE_WIDTH,
  DIFF_GHOST_OPACITY,
  DIFF_STATUS_COLOR_VAR,
  ENUM_CARD_W,
} from '../constants';
import { CARD_W, cardH } from '../glass';
import { t } from '../i18n';
import { iconSvg } from '../icons';
import { escHtml } from '../panels';
import { curvedEdge } from '../edge-path';
import { qualifiedTableName, type EnumDef, type Table } from '../types';
import { enumCardH, type GhostEdge, type GhostSlot, type Pt } from './layout';
import {
  DIFF_GROUPS,
  STATUS_MARK,
  formatFieldChange,
  groupCounts,
  isEmptyDiff,
  type ColumnDiff,
  type DiffGroup,
  type DiffItem,
  type DiffStatus,
  type DiffView,
  type EnumDiff,
  type TableDiff,
} from './model';
import { diffErrorMessageKey, type DiffState } from './state';

export const diffStatusClass = (status: DiffStatus): string => `diff-${status}`;

const STATUS_WORD: Record<DiffStatus, string> = { added: 'adicionado', modified: 'modificado', removed: 'removido' };

const GROUP_TITLE: Record<DiffGroup, string> = {
  tables: 'TABELAS',
  columns: 'COLUNAS',
  indexes: 'ÍNDICES',
  enums: 'ENUMS',
  refs: 'RELAÇÕES',
};

/** Marcador de texto (`+`, `~`, `−`) colorido pelo status. */
export const diffMark = (status: DiffStatus): string =>
  `<span class="dmark" style="color:${DIFF_STATUS_COLOR_VAR[status]}" aria-hidden="true">${STATUS_MARK[status]}</span>`;
const mark = diffMark;

/** Nome traduzido do status, para rótulos de acessibilidade. */
export const diffStatusLabel = (status: DiffStatus): string => t(STATUS_WORD[status]);

/** Título (tooltip) de uma coluna modificada: `int → bigint, +not null`. Vazio nas demais. */
export function columnDiffTitle(cd: ColumnDiff): string {
  return cd.status === 'modified' ? cd.changes.map(formatFieldChange).join(', ') : '';
}

const phead = (): string =>
  `<div class="phead">${escHtml(t('Comparar com o git HEAD'))}<span class="grow"></span>
    <button class="ib sm" data-act="${DIFF_ACT.EXIT}" title="${escHtml(t('Sair do modo Comparar'))}" aria-label="${escHtml(t('Sair do modo Comparar'))}">${iconSvg('X', 14)}</button></div>`;

const summaryChips = (view: DiffView): string =>
  (['added', 'modified', 'removed'] as const)
    .map((status) => {
      const label = t(`{} ${STATUS_PLURAL[status]}`, view.summary[status]);
      return `<span class="dchip ${diffStatusClass(status)}" style="color:${DIFF_STATUS_COLOR_VAR[status]}">${STATUS_MARK[status]} ${escHtml(label)}</span>`;
    })
    .join('');

const STATUS_PLURAL: Record<DiffStatus, string> = { added: 'adicionados', modified: 'modificados', removed: 'removidos' };

const row = (item: DiffItem): string => {
  const word = t(STATUS_WORD[item.status]);
  const detail = item.detail ? `<span class="cnt-mono" title="${escHtml(item.detail)}">${escHtml(item.detail)}</span>` : '';
  return `<button type="button" class="trow diff-row ${diffStatusClass(item.status)}" data-act="${DIFF_ACT.FOCUS}" data-diff-id="${escHtml(item.id)}" aria-label="${escHtml(`${word}: ${item.label}`)}">
    ${mark(item.status)}<span class="nm">${escHtml(item.label)}</span><span class="grow"></span>${detail}</button>`;
};

function groups(view: DiffView): string {
  const counts = groupCounts(view);
  return DIFF_GROUPS.filter((g) => counts[g] > 0)
    .map(
      (g) => `<div class="psec" data-diff-group="${g}"><div class="pshd"><span>${escHtml(t(GROUP_TITLE[g]))}</span><span class="pill">${counts[g]}</span></div>
      ${view.items.filter((i) => i.group === g).map(row).join('')}</div>`
    )
    .join('');
}

const note = (iconHtml: string, text: string, extra = ''): string =>
  `<div class="pempty diff-note" role="status">${iconHtml}<span>${escHtml(text)}</span>${extra}</div>`;

/** Conteúdo do painel lateral do modo Comparar para cada estado (carregando, erro, vazio, resumo). */
export function renderDiffPanel(state: DiffState): string {
  let body = '';
  if (state.status === 'loading') {
    body = note(iconSvg('Clock', 14), t('Carregando a versão do git HEAD…'));
  } else if (state.status === 'error') {
    const detail = state.detail ? `<div class="sub diff-detail">${escHtml(state.detail)}</div>` : '';
    const retry = `<button type="button" class="btn sm" data-act="${DIFF_ACT.RETRY}">${escHtml(t('Tentar novamente'))}</button>`;
    body = `<div class="pempty diff-note diff-error" role="alert">${iconSvg('TriangleAlert', 14, 'var(--warning)')}<div style="min-width:0"><div>${escHtml(t(diffErrorMessageKey(state.code)))}</div>${detail}<div style="margin-top:8px">${retry}</div></div></div>`;
  } else if (state.status === 'ready') {
    body = isEmptyDiff(state.view)
      ? note(iconSvg('CircleCheck', 14, 'var(--ok)'), t('Sem diferenças em relação ao git HEAD.'))
      : `<div class="diff-summary" role="status">${summaryChips(state.view)}</div>${groups(state.view)}`;
  }
  return `${phead()}<div class="pbody diff-body">${body}<div class="grow" style="border-top:1px solid var(--edge-soft)"></div></div>`;
}

/** Cartões fantasma a posicionar (`placeGhosts`): tabelas e enums removidos. */
export function ghostSlots(view: DiffView): GhostSlot[] {
  return [
    ...view.removedTables.map((tbl) => ({ key: qualifiedTableName(tbl), w: CARD_W, h: cardH(tbl.columns.length) })),
    ...view.removedEnums.map((e) => ({ key: `enum:${e.name}`, w: ENUM_CARD_W, h: enumCardH(e.values.length) })),
  ];
}

const ghostStyle = (pos: Pt, height: number | null): string =>
  `left:${pos.x}px;top:${pos.y}px;${height === null ? '' : `height:${height}px;`}opacity:${DIFF_GHOST_OPACITY};border-color:${DIFF_STATUS_COLOR_VAR.removed};border-style:dashed`;

/** Cartão fantasma (translúcido, tracejado) de uma tabela removida; sem `data-table` (não seleciona nem arrasta). */
export function renderGhostTable(tbl: Table, pos: Pt): string {
  const qname = qualifiedTableName(tbl);
  const rows = tbl.columns
    .map(
      (c) => `<div class="row diff-removed" role="listitem"><div class="rail14">${mark('removed')}</div>
      <div class="cn">${escHtml(c.name)}</div><div class="ty">${escHtml(c.ty)}</div></div>`
    )
    .join('');
  const label = t('Tabela removida {}', qname);
  return `<div class="card diff-ghost diff-removed" role="group" aria-label="${escHtml(label)}" data-diff-ghost="${escHtml(qname)}" style="${ghostStyle(pos, cardH(tbl.columns.length))}">
    <div class="head"><span style="color:${DIFF_STATUS_COLOR_VAR.removed};display:flex">${iconSvg('Table2', 14)}</span>
    <div class="nm">${escHtml(tbl.name)}</div><div class="cnt">${tbl.columns.length}</div></div>
    <div class="rows" role="list">${rows}</div></div>`;
}

/** Cartão fantasma de um enum removido. */
export function renderGhostEnum(def: EnumDef, pos: Pt): string {
  const values = def.values
    .map((v) => `<div class="ev diff-removed" role="listitem"><span class="ev-ic">${mark('removed')}</span>${escHtml(v.name)}</div>`)
    .join('');
  const label = t('Enum removido {}', def.name);
  return `<div class="card enum-card diff-ghost diff-removed" role="group" aria-label="${escHtml(label)}" data-diff-ghost="${escHtml(`enum:${def.name}`)}" style="${ghostStyle(pos, null)}">
    <div class="head"><span style="color:${DIFF_STATUS_COLOR_VAR.removed};display:flex">${iconSvg('List', 14)}</span>
    <div class="nm">${escHtml(def.name)}</div><span class="tag">ENUM</span></div>
    <div class="rows" role="list">${values}</div></div>`;
}

/** Linhas fantasma (colunas removidas) anexadas ao fim do cartão de uma tabela do head. */
export function renderGhostRows(diff: TableDiff): string {
  return diff.ghostColumns
    .map((c) => {
      const label = t('Coluna removida {}', c.name);
      return `<div class="row diff-removed diff-ghost-row" role="listitem" aria-label="${escHtml(label)}" data-diff-ghost-col="${escHtml(c.name)}"><div class="rail14">${mark('removed')}</div>
        <div class="cn">${escHtml(c.name)}</div><div class="ty">${escHtml(c.ty)}</div></div>`;
    })
    .join('');
}

/** Valores removidos de um enum do head, anexados ao fim do cartão. */
export function renderEnumGhostValues(diff: EnumDiff): string {
  return diff.ghostValues
    .map((v) => `<div class="ev diff-removed diff-ghost-row" role="listitem"><span class="ev-ic">${mark('removed')}</span>${escHtml(v)}</div>`)
    .join('');
}

/** Arestas de relações removidas: tracejadas e vermelhas, sob os cartões. */
export function renderGhostEdges(edges: readonly GhostEdge[]): string {
  return edges
    .map((e) => {
      const curve = curvedEdge(e.ax, e.ay, e.bx, e.by);
      return `<g class="diff-ghost-edge" data-diff-ref="${escHtml(e.key)}"><path d="${curve.path}" fill="none" stroke="${DIFF_STATUS_COLOR_VAR.removed}" stroke-width="${DIFF_GHOST_EDGE_WIDTH}" stroke-dasharray="${DIFF_GHOST_DASH}" stroke-linejoin="round"/>
        <circle cx="${curve.x1}" cy="${e.ay}" r="3" fill="${DIFF_STATUS_COLOR_VAR.removed}"/><circle cx="${curve.x2}" cy="${e.by}" r="3" fill="${DIFF_STATUS_COLOR_VAR.removed}"/></g>`;
    })
    .join('');
}

