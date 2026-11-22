/**
 * Geometria do modo Comparar: posiciona cartões fantasma (tabelas/enums removidos) em espaço
 * livre e resolve a linha de uma coluna para desenhar as arestas removidas. Módulo puro.
 */

import { ENUM_CARD_HEAD_H, ENUM_CARD_PAD_H, ENUM_CARD_ROW_H } from '../constants';
import { CARD_W, HEADER_H, ROW_H, cardH } from '../glass';
import { qualifiedEndpointTable, qualifiedTableName, type Ref, type Table } from '../types';
import type { DiffView } from './model';

export interface Pt {
  x: number;
  y: number;
}

export interface Rect extends Pt {
  w: number;
  h: number;
}

export interface GhostSlot {
  key: string;
  w: number;
  h: number;
  /** Posição conhecida (ex.: sidecar antigo); só é usada se estiver livre. */
  preferred?: Pt;
}

export const GHOST_GAP = 40;
const ROW_MAX_WIDTH = 4 * (CARD_W + GHOST_GAP);

const overlaps = (a: Rect, b: Rect, gap: number): boolean =>
  a.x < b.x + b.w + gap && b.x < a.x + a.w + gap && a.y < b.y + b.h + gap && b.y < a.y + a.h + gap;

/**
 * Escolhe posições para os cartões fantasma sem sobrepor `occupied`: a posição preferida quando
 * livre; senão, fileiras abaixo do diagrama (alinhadas à esquerda do conteúdo existente).
 */
export function placeGhosts(occupied: readonly Rect[], slots: readonly GhostSlot[], gap = GHOST_GAP): Record<string, Pt> {
  const placed: Rect[] = [...occupied];
  const out: Record<string, Pt> = {};
  const left = occupied.length ? Math.min(...occupied.map((r) => r.x)) : 0;
  const bottom = occupied.length ? Math.max(...occupied.map((r) => r.y + r.h)) : 0;
  let x = left;
  let y = occupied.length ? bottom + 2 * gap : 0;
  let rowH = 0;

  for (const slot of slots) {
    const pref = slot.preferred;
    if (pref && !placed.some((r) => overlaps({ ...pref, w: slot.w, h: slot.h }, r, 0))) {
      out[slot.key] = { ...pref };
      placed.push({ ...pref, w: slot.w, h: slot.h });
      continue;
    }
    if (x > left && x + slot.w > left + ROW_MAX_WIDTH) {
      x = left;
      y += rowH + gap;
      rowH = 0;
    }
    out[slot.key] = { x, y };
    placed.push({ x, y, w: slot.w, h: slot.h });
    x += slot.w + gap;
    rowH = Math.max(rowH, slot.h);
  }
  return out;
}

/** Y (mundo) do centro da linha `row` de um cartão posicionado em `pos`. */
export const rowCenterY = (pos: Pt, row: number): number => pos.y + HEADER_H + 4 + ROW_H * (row + 0.5);

/** Altura de um cartão de enum com `values` valores (mínimo de uma linha, para o "(vazio)"). */
export const enumCardH = (values: number): number => ENUM_CARD_HEAD_H + ENUM_CARD_ROW_H * Math.max(values, 1) + ENUM_CARD_PAD_H;

/** Altura do cartão com as colunas fantasma (removidas) anexadas ao fim. */
export function diffCardHeight(table: Table, view: DiffView | null): number {
  const ghost = view?.tables.get(qualifiedTableName(table))?.ghostColumns.length ?? 0;
  return cardH(table.columns.length + ghost);
}

/**
 * Índice de linha da coluna no cartão desenhado: colunas do head primeiro, depois as removidas
 * (fantasma) e, para tabelas removidas, as colunas da base. `null` se a coluna não existe.
 */
export function columnRow(qname: string, column: string, headTables: readonly Table[], view: DiffView): number | null {
  const head = headTables.find((t) => qualifiedTableName(t) === qname);
  if (head) {
    const i = head.columns.findIndex((c) => c.name === column);
    if (i >= 0) return i;
    const g = view.tables.get(qname)?.ghostColumns.findIndex((c) => c.name === column) ?? -1;
    return g >= 0 ? head.columns.length + g : null;
  }
  const removed = view.removedTables.find((t) => qualifiedTableName(t) === qname);
  if (!removed) return null;
  const i = removed.columns.findIndex((c) => c.name === column);
  return i >= 0 ? i : null;
}

export interface GhostEdge {
  key: string;
  ref: Ref;
  from: string;
  to: string;
  ax: number;
  ay: number;
  bx: number;
  by: number;
}

/**
 * Geometria das relações removidas. Só entram as que têm as duas pontas desenháveis (tabela com
 * posição e coluna conhecida); as demais continuam listadas no painel.
 */
export function removedEdges(
  view: DiffView,
  headTables: readonly Table[],
  positions: Record<string, Pt>,
  ghostPositions: Record<string, Pt>
): GhostEdge[] {
  const out: GhostEdge[] = [];
  const at = (qname: string): Pt | undefined => positions[qname] ?? ghostPositions[qname];
  for (const d of view.refs.values()) {
    if (d.status !== 'removed') continue;
    const from = qualifiedEndpointTable(d.ref.from);
    const to = qualifiedEndpointTable(d.ref.to);
    const a = at(from);
    const b = at(to);
    const ra = columnRow(from, d.ref.from.column, headTables, view);
    const rb = columnRow(to, d.ref.to.column, headTables, view);
    if (!a || !b || ra === null || rb === null) continue;
    out.push({ key: d.key, ref: d.ref, from, to, ax: a.x, ay: rowCenterY(a, ra), bx: b.x, by: rowCenterY(b, rb) });
  }
  return out;
}

/** Retângulos ocupados pelos cartões do head (para posicionar fantasmas fora deles). */
export function occupiedRects(tables: readonly Table[], positions: Record<string, Pt>, view: DiffView | null): Rect[] {
  const out: Rect[] = [];
  for (const t of tables) {
    const p = positions[qualifiedTableName(t)];
    if (p) out.push({ x: p.x, y: p.y, w: CARD_W, h: diffCardHeight(t, view) });
  }
  return out;
}
