/**
 * Camada de tema e constantes visuais portada 1:1 de `crates/app/src/glass.rs`
 * e `crates/app/src/canvas.rs` / `crates/app/src/shell/mod.rs`.
 */

export const CARD_W = 260;
export const HEADER_H = 44;
export const ROW_H = 30;
export const MIN_ZOOM = 0.2;
export const MAX_ZOOM = 2.5;
export const GROUP_PAD = 32;

export const RADIUS_CARD = 12;
export const RADIUS_ISLAND = 10;

export const SP_SM = 8;
export const SP_MD = 12;
export const SP_LG = 16;

export const TEXT_XS = 12;
export const TEXT_SM = 13.5;
export const TEXT_BASE = 14.5;
export const TEXT_LG = 16.5;

export const TITLEBAR_H = 38;
export const STATUSBAR_H = 24;
export const RAIL_W = 44;
export const SIDEBAR_W = 260;
export const INSPECTOR_W = 300;
export const EDITOR_W = 460;

export function cardH(rows: number): number {
  return HEADER_H + ROW_H * rows + 8;
}

/** Paleta para cor de tabelas e grupos (`crates/app/src/glass.rs::TABLE_COLORS`) */
export const TABLE_COLORS: ReadonlyArray<readonly [string, string]> = [
  ['Blue', '#3b82f6'],
  ['Cyan', '#06b6d4'],
  ['Green', '#22c55e'],
  ['Lime', '#84cc16'],
  ['Amber', '#f59e0b'],
  ['Orange', '#f97316'],
  ['Pink', '#ec4899'],
  ['Violet', '#8b5cf6'],
];

/** Tipos padrão sugeridos no autocomplete do Inspector (`crates/app/src/shell/mod.rs::STANDARD_TYPES`) */
export const STANDARD_TYPES: ReadonlyArray<readonly [string, string]> = [
  ['int', '4-byte integer'],
  ['bigint', '8-byte integer'],
  ['smallint', '2-byte integer'],
  ['serial', 'auto-increment'],
  ['varchar(255)', 'variable-length text'],
  ['text', 'long text'],
  ['boolean', 'boolean'],
  ['uuid', 'unique identifier'],
  ['timestamp', 'date and time'],
  ['timestamptz', 'date/time with timezone'],
  ['date', 'date'],
  ['time', 'time'],
  ['decimal(10,2)', 'exact decimal'],
  ['float', 'floating point'],
  ['double', 'double precision'],
  ['json', 'JSON'],
  ['jsonb', 'binary JSON'],
  ['bytea', 'binary'],
];

export type Mode = 'dark' | 'light';

export interface Glass {
  mode: Mode;
  uiScale: number;
  chrome: string;
  sidebar: string;
  surface: string;
  grid: string;
  card: string;
  cardTranslucent: string;
  island: string;
  islandSheen: string;
  edge: string;
  edgeSoft: string;
  hover: string;
  active: string;
  selected: string;
  text: string;
  textMuted: string;
  textFaint: string;
  accent: string;
  danger: string;
  warning: string;
  ok: string;
  cardShadow: string;
  cardShadowSelected: string;
  islandShadow: string;
}

export function getGlass(mode: Mode, uiScale = 1.0): Glass {
  if (mode === 'dark') {
    return {
      mode: 'dark',
      uiScale,
      chrome: '#131314',
      sidebar: '#101011',
      surface: '#0a0a0b',
      grid: 'rgba(255, 255, 255, 0.07)',
      card: '#151516',
      cardTranslucent: 'rgba(21, 21, 22, 0.55)',
      island: 'rgba(22, 22, 23, 0.90)',
      islandSheen: 'rgba(255, 255, 255, 0.14)',
      edge: 'rgba(255, 255, 255, 0.10)',
      edgeSoft: 'rgba(255, 255, 255, 0.05)',
      hover: 'rgba(255, 255, 255, 0.06)',
      active: 'rgba(255, 255, 255, 0.10)',
      selected: 'rgba(198, 242, 64, 0.18)',
      text: '#ececed',
      textMuted: '#a3a6ad',
      textFaint: '#72757d',
      accent: '#c6f240',
      danger: '#f2555a',
      warning: '#e5b454',
      ok: '#4cc27a',
      cardShadow: '0 2px 8px 0 rgba(0, 0, 0, 0.25)',
      cardShadowSelected: '0 2px 8px 0 rgba(0, 0, 0, 0.25), 0 0 0 3px rgba(198, 242, 64, 0.25)',
      islandShadow: '0 6px 18px -4px rgba(0, 0, 0, 0.42), 0 1px 2px 0 rgba(0, 0, 0, 0.25)',
    };
  }

  return {
    mode: 'light',
    uiScale,
    chrome: '#e4e6ea',
    sidebar: '#eef0f3',
    surface: '#f7f7f8',
    grid: 'rgba(31, 35, 40, 0.13)',
    card: '#ffffff',
    cardTranslucent: 'rgba(255, 255, 255, 0.55)',
    island: 'rgba(255, 255, 255, 0.95)',
    islandSheen: '#ffffff',
    edge: 'rgba(31, 35, 40, 0.13)',
    edgeSoft: 'rgba(31, 35, 40, 0.08)',
    hover: 'rgba(31, 35, 40, 0.05)',
    active: 'rgba(31, 35, 40, 0.09)',
    selected: 'rgba(95, 125, 0, 0.12)',
    text: '#1f2328',
    textMuted: '#57606a',
    textFaint: '#848d97',
    accent: '#5f7d00',
    danger: '#cf222e',
    warning: '#9a6700',
    ok: '#1a7f37',
    cardShadow: '0 2px 8px 0 rgba(0, 0, 0, 0.06)',
    cardShadowSelected: '0 2px 8px 0 rgba(0, 0, 0, 0.06), 0 0 0 3px rgba(95, 125, 0, 0.25)',
    islandShadow: '0 6px 18px -4px rgba(0, 0, 0, 0.12), 0 1px 2px 0 rgba(0, 0, 0, 0.07)',
  };
}

/** Converte `#rrggbb` + opacidade `0..1` em `rgba(r, g, b, a)` */
export function hexWithAlpha(hexColor: string, alpha: number): string {
  const clean = hexColor.trim().replace(/^#/, '');
  if (clean.length !== 6) return hexColor;
  const num = parseInt(clean, 16);
  if (Number.isNaN(num)) return hexColor;
  const r = (num >> 16) & 255;
  const g = (num >> 8) & 255;
  const b = num & 255;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}
