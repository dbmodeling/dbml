/**
 * Camada de tema e constantes visuais (protótipo "DB Modeling v5"), espelho de
 * `crates/app/src/glass.rs`. Cores cruas só existem aqui.
 */

export const WHITE = '#fff';
export const WHITE_30 = 'rgba(255,255,255,.3)';
export const WHITE_20 = 'rgba(255,255,255,.2)';
export const WHITE_75 = 'rgba(255,255,255,.75)';
export const BLACK_20 = 'rgba(0,0,0,.2)';
export const BLACK_30 = 'rgba(0,0,0,.3)';

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

export const TITLEBAR_H = 42;
export const STATUSBAR_H = 26;
export const RAIL_W = 48;
export const SIDEBAR_W = 248;
export const INSPECTOR_W = 288;
export const EDITOR_W = 440;

export function cardH(rows: number): number {
  return HEADER_H + ROW_H * rows + 8;
}

/** Paleta para cor de tabelas e grupos (`crates/app/src/glass.rs::TABLE_COLORS`) */
export const TABLE_COLORS: ReadonlyArray<readonly [string, string]> = [
  ['Azul', '#589aed'],
  ['Ciano', '#00b1ba'],
  ['Verde', '#4eb068'],
  ['Lima', '#8ea432'],
  ['Âmbar', '#c09000'],
  ['Laranja', '#df784e'],
  ['Rosa', '#d771a4'],
  ['Violeta', '#9f85e5'],
];

/** Tipos padrão sugeridos no autocomplete do Inspector (`crates/app/src/shell/mod.rs::STANDARD_TYPES`, em pt-BR) */
export const STANDARD_TYPES: ReadonlyArray<readonly [string, string]> = [
  ['int', 'inteiro de 4 bytes'],
  ['bigint', 'inteiro de 8 bytes'],
  ['smallint', 'inteiro de 2 bytes'],
  ['serial', 'autoincremento'],
  ['varchar(255)', 'texto de tamanho variável'],
  ['text', 'texto longo'],
  ['boolean', 'booleano'],
  ['uuid', 'identificador único'],
  ['timestamp', 'data e hora'],
  ['timestamptz', 'data/hora com fuso'],
  ['date', 'data'],
  ['time', 'hora'],
  ['decimal(10,2)', 'decimal exato'],
  ['float', 'ponto flutuante'],
  ['double', 'precisão dupla'],
  ['json', 'JSON'],
  ['jsonb', 'JSON binário'],
  ['bytea', 'binário'],
];

export type Mode = 'dark' | 'light';

export interface Glass {
  mode: Mode;
  uiScale: number;
  wall: string;
  blur: string;
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
  /** Trilho das barras (atividades, status) e fundo/calha do editor. */
  rail: string;
  editor: string;
  gutter: string;
  lnum: string;
  /** Seleção neutra (abas, segmentados), campos e contorno forte (`line2`). */
  sel: string;
  selShadow: string;
  input: string;
  inputSolid: string;
  edgeStrong: string;
  /** Popovers, diálogos e véu atrás deles. */
  pop: string;
  modal: string;
  scrim: string;
  popShadow: string;
  /** Relações no canvas e realce de sintaxe DBML. */
  relLine: string;
  relStroke: string;
  relLabel: string;
  link: string;
  idx: string;
  swatchRing: string;
  synKw: string;
  synType: string;
  synAttr: string;
  synNum: string;
  synComment: string;
  synPunct: string;
}

/** Tamanho real do "100%" da escala de UI: o antigo 130% virou o normal (igual a `glass.rs`). */
export const UI_BASE = 1.3;

export function getGlass(mode: Mode, uiScale = 1.0, glass = false): Glass {
  const accent = '#e56731';
  if (mode === 'dark') {
    return {
      mode: 'dark',
      uiScale,
      wall: 'radial-gradient(55% 55% at 15% 20%,#1a2a35,transparent),radial-gradient(45% 50% at 85% 80%,#3a2216,transparent),radial-gradient(40% 40% at 60% 10%,#1d2a1c,transparent),#08090a',
      blur: glass ? 'blur(48px) saturate(170%)' : 'none',
      chrome: glass ? 'rgba(14,15,17,.62)' : '#0e0f11',
      sidebar: glass ? 'rgba(18,19,22,.55)' : '#121316',
      surface: glass ? 'rgba(6,7,8,.5)' : '#0a0b0c',
      grid: 'rgba(255, 255, 255, 0.085)',
      card: glass ? 'rgba(24,25,28,.96)' : '#17181b',
      cardTranslucent: 'rgba(30, 31, 35, 0.8)',
      island: 'rgba(28, 29, 32, 0.86)',
      islandSheen: 'rgba(255, 255, 255, 0.14)',
      edge: 'rgba(255, 255, 255, 0.10)',
      edgeSoft: 'rgba(255, 255, 255, 0.07)',
      hover: 'rgba(255, 255, 255, 0.06)',
      active: 'rgba(255, 255, 255, 0.09)',
      selected: 'rgba(229, 103, 49, 0.18)',
      text: '#ececee',
      textMuted: '#a3a5ab',
      textFaint: '#858890',
      accent,
      danger: '#ed4a49',
      warning: '#eba941',
      ok: '#3eab5e',
      cardShadow: '0 10px 30px rgba(0, 0, 0, 0.35)',
      cardShadowSelected: '0 0 0 3px rgba(229, 103, 49, 0.18), 0 10px 30px rgba(0, 0, 0, 0.35)',
      islandShadow: '0 10px 30px rgba(0, 0, 0, 0.4)',
      rail: glass ? 'rgba(11,12,14,.55)' : '#0b0c0e',
      editor: glass ? 'rgba(6,7,8,.6)' : '#0a0b0c',
      gutter: 'rgba(255, 255, 255, 0.02)',
      lnum: '#41444a',
      sel: 'rgba(255, 255, 255, 0.09)',
      selShadow: 'inset 0 1px 0 rgba(255, 255, 255, 0.06)',
      input: 'rgba(255, 255, 255, 0.05)',
      inputSolid: 'rgba(0, 0, 0, 0.3)',
      edgeStrong: 'rgba(255, 255, 255, 0.12)',
      pop: 'rgba(30, 31, 35, 0.94)',
      modal: 'rgba(24, 25, 28, 0.94)',
      scrim: 'rgba(0, 0, 0, 0.4)',
      popShadow: '0 30px 80px rgba(0, 0, 0, 0.55), inset 0 1px 0 rgba(255, 255, 255, 0.06)',
      relLine: 'rgba(220, 225, 235, 0.42)',
      relStroke: 'rgba(220, 225, 235, 0.32)',
      relLabel: '#858890',
      link: '#589aed',
      idx: '#00b1ba',
      swatchRing: '#121316',
      synKw: '#7cb4fc',
      synType: '#64d1d7',
      synAttr: '#e39bdc',
      synNum: '#f6b669',
      synComment: '#5d6067',
      synPunct: '#8b8e95',
    };
  }

  return {
    mode: 'light',
    uiScale,
    wall: 'radial-gradient(60% 60% at 15% 15%,#c9dce6,transparent),radial-gradient(50% 55% at 85% 85%,#d6e2c2,transparent),radial-gradient(40% 40% at 80% 15%,#efd9c4,transparent),#e8e5dc',
    blur: glass ? 'blur(48px) saturate(170%)' : 'none',
    chrome: glass ? 'rgba(246,246,244,.62)' : '#f3f3f1',
    sidebar: glass ? 'rgba(247,247,245,.55)' : '#f7f7f5',
    surface: glass ? 'rgba(255,255,255,.55)' : '#fbfbfa',
    grid: 'rgba(0, 0, 0, 0.12)',
    card: '#ffffff',
    cardTranslucent: 'rgba(255, 255, 255, 0.8)',
    island: 'rgba(255, 255, 255, 0.86)',
    islandSheen: '#ffffff',
    edge: 'rgba(20, 20, 30, 0.12)',
    edgeSoft: 'rgba(20, 20, 30, 0.08)',
    hover: 'rgba(0, 0, 0, 0.05)',
    active: 'rgba(0, 0, 0, 0.09)',
    selected: 'rgba(229, 103, 49, 0.12)',
    text: '#17181b',
    textMuted: '#55585f',
    textFaint: '#6b6e75',
    accent,
    danger: '#ed4a49',
    warning: '#b97a12',
    ok: '#3eab5e',
    cardShadow: '0 1px 2px rgba(0, 0, 0, 0.05), 0 6px 18px rgba(0, 0, 0, 0.05)',
    cardShadowSelected:
      '0 0 0 3px rgba(229, 103, 49, 0.18), 0 1px 2px rgba(0, 0, 0, 0.05), 0 6px 18px rgba(0, 0, 0, 0.05)',
    islandShadow: '0 8px 24px rgba(0, 0, 0, 0.08)',
    rail: glass ? 'rgba(236,236,234,.55)' : '#ececea',
    editor: glass ? 'rgba(255,255,255,.6)' : '#ffffff',
    gutter: 'rgba(0, 0, 0, 0.025)',
    lnum: '#b9bbc0',
    sel: '#ffffff',
    selShadow: '0 1px 0 rgba(0, 0, 0, 0.04), 0 1px 3px rgba(0, 0, 0, 0.1)',
    input: glass ? 'rgba(0, 0, 0, 0.045)' : '#ebebe8',
    inputSolid: '#ffffff',
    edgeStrong: 'rgba(20, 20, 30, 0.13)',
    pop: 'rgba(255, 255, 255, 0.92)',
    modal: 'rgba(252, 252, 251, 0.94)',
    scrim: 'rgba(30, 30, 40, 0.14)',
    popShadow: '0 24px 60px rgba(30, 30, 40, 0.2), 0 2px 6px rgba(0, 0, 0, 0.06)',
    relLine: 'rgba(30, 30, 40, 0.42)',
    relStroke: 'rgba(30, 30, 40, 0.3)',
    relLabel: '#6b6e75',
    link: '#589aed',
    idx: '#00b1ba',
    swatchRing: '#f6f6f4',
    synKw: '#2663c9',
    synType: '#007d86',
    synAttr: '#a04c9a',
    synNum: '#ab5900',
    synComment: '#9a9ca2',
    synPunct: '#6b6e75',
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
