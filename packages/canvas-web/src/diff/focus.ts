/**
 * Foco por clique no resumo do modo Comparar: acha o retângulo (espaço do mundo) do item e calcula
 * a câmera (pan/zoom) que o enquadra. Módulo puro.
 */

import { DIFF_FOCUS_MIN_ZOOM, DIFF_FOCUS_PAD, ENUM_CARD_W } from '../constants';
import { CARD_W, MAX_ZOOM, MIN_ZOOM, cardH } from '../glass';
import { qualifiedEndpointTable, qualifiedTableName, type Table } from '../types';
import { diffCardHeight, enumCardH, type Pt, type Rect } from './layout';
import type { DiffItem, DiffView } from './model';

export interface FocusContext {
  view: DiffView;
  /** Tabelas do head. */
  tables: readonly Table[];
  /** Posições dos cartões do head (`qname`, `enum:<nome>`). */
  positions: Record<string, Pt>;
  /** Posições escolhidas para os cartões fantasma (`qname`, `enum:<nome>`). */
  ghostPositions: Record<string, Pt>;
  /** Retângulos dos enums do head (nome → retângulo), vindos da cena. */
  enumRects: ReadonlyMap<string, Rect>;
}

const union = (a: Rect, b: Rect): Rect => {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
};

function tableRect(qname: string, ctx: FocusContext): Rect | null {
  const head = ctx.tables.find((t) => qualifiedTableName(t) === qname);
  const pos = ctx.positions[qname];
  if (head && pos) return { ...pos, w: CARD_W, h: diffCardHeight(head, ctx.view) };
  const removed = ctx.view.removedTables.find((t) => qualifiedTableName(t) === qname);
  const ghost = ctx.ghostPositions[qname];
  return removed && ghost ? { ...ghost, w: CARD_W, h: cardH(removed.columns.length) } : null;
}

function enumRect(name: string, removed: boolean, ctx: FocusContext): Rect | null {
  if (!removed) return ctx.enumRects.get(name) ?? null;
  const def = ctx.view.removedEnums.find((e) => e.name === name);
  const ghost = ctx.ghostPositions[`enum:${name}`];
  return def && ghost ? { ...ghost, w: ENUM_CARD_W, h: enumCardH(def.values.length) } : null;
}

/** Retângulo do item do resumo, ou `null` se ele não tem posição conhecida (não é desenhado). */
export function focusRect(item: DiffItem, ctx: FocusContext): Rect | null {
  const target = item.target;
  switch (target.kind) {
    case 'table':
      return tableRect(target.name, ctx);
    case 'enum':
      return enumRect(target.name, target.removed, ctx);
    case 'ref': {
      const d = ctx.view.refs.get(target.key);
      if (!d) return null;
      const a = tableRect(qualifiedEndpointTable(d.ref.from), ctx);
      const b = tableRect(qualifiedEndpointTable(d.ref.to), ctx);
      return a && b ? union(a, b) : (a ?? b);
    }
  }
}

export interface Camera {
  zoom: number;
  pan: Pt;
}

/**
 * Câmera que centraliza `rect` numa vista de `viewport` px. Mantém o zoom atual (sobe para 1 se
 * estiver muito afastado, como `focusTable`) e só reduz se o retângulo não couber.
 */
export function cameraFor(rect: Rect, viewport: Pt, currentZoom: number): Camera {
  let zoom = currentZoom < DIFF_FOCUS_MIN_ZOOM ? 1 : currentZoom;
  const fit = Math.min(viewport.x / (rect.w + 2 * DIFF_FOCUS_PAD), viewport.y / (rect.h + 2 * DIFF_FOCUS_PAD));
  zoom = Math.max(MIN_ZOOM, Math.min(zoom, fit, MAX_ZOOM));
  return {
    zoom,
    pan: { x: viewport.x / 2 - (rect.x + rect.w / 2) * zoom, y: viewport.y / 2 - (rect.y + rect.h / 2) * zoom },
  };
}
