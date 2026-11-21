/**
 * Renderizadores dos elementos internos da superfície do Canvas (`crates/app/src/canvas.rs`):
 * Molduras de Grupo (`TableGroup`), Relações SVG, Cartões de Tabela DOM e Ilhas Flutuantes.
 */

import { curvedEdge, edgeDrawing } from './edge-path';
import { Schema, SceneGeometry, EdgeStyle, edgeKey, qualifiedEndpointTable, qualifiedTableName } from './types';
import { Point } from './viewport';
import {
  Glass,
  CARD_W,
  HEADER_H,
  ROW_H,
  GROUP_PAD,
  TABLE_COLORS,
  cardH,
  hexWithAlpha,
} from './glass';
import { iconSvg } from './icons';
import { escHtml, renderKbd } from './panels';

export interface ActiveLinkDrag {
  fromTable: string;
  fromColumn: string;
  startX: number;
  startY: number;
  nowX: number;
  nowY: number;
}

export interface ActiveMarqueeDrag {
  startX: number;
  startY: number;
  nowX: number;
  nowY: number;
}

/** Renderiza as molduras de grupos, arestas SVG e cartões de tabela dentro do container `.dbm-world` */
export function renderCanvasWorld(params: {
  schema: Schema;
  positions: Record<string, Point>;
  scene: SceneGeometry | null;
  edges?: Record<string, EdgeStyle>;
  selection: string[];
  topTable: string | null;
  linkDrag: ActiveLinkDrag | null;
  marqueeDrag: ActiveMarqueeDrag | null;
  tool: 'select' | 'hand';
  g: Glass;
}): string {
  const {
    schema,
    positions,
    scene,
    edges: edgeStyles,
    selection,
    topTable,
    linkDrag,
    marqueeDrag,
    tool,
    g,
  } = params;

  const isSelected = (name: string) => selection.includes(name);
  let html = '';

  // 1. Molduras de Grupo (`canvas.rs` linhas 772-855)
  const groups =
    scene?.groups ??
    schema.groups
      .map((gr, idx) => {
        const members = gr.tables
          .map((tn) => ({ pos: positions[tn], tbl: schema.tables.find((x) => x.name === tn) }))
          .filter((m) => m.pos && m.tbl);
        if (members.length === 0) return null;
        let x0 = Infinity;
        let y0 = Infinity;
        let x1 = -Infinity;
        let y1 = -Infinity;
        for (const m of members) {
          x0 = Math.min(x0, m.pos!.x);
          y0 = Math.min(y0, m.pos!.y);
          x1 = Math.max(x1, m.pos!.x + CARD_W);
          y1 = Math.max(y1, m.pos!.y + cardH(m.tbl!.columns.length));
        }
        const pad = GROUP_PAD - 8;
        const color = gr.color || TABLE_COLORS[idx % TABLE_COLORS.length][1];
        return {
          name: gr.name,
          x: x0 - pad,
          y: y0 - pad - 8,
          w: x1 - x0 + 2 * pad,
          h: y1 - y0 + 2 * pad + 8,
          color,
          count: members.length,
          tables: gr.tables,
        };
      })
      .filter(Boolean);

  for (const gr of groups) {
    if (!gr) continue;
    html += `
      <div class="dbm-group" style="left:${gr.x}px;top:${gr.y}px;width:${gr.w}px;height:${gr.h}px;border-color:${hexWithAlpha(gr.color, 0.4)};background:${hexWithAlpha(gr.color, 0.06)};">
        <div class="dbm-group-label" data-drag-group="${escHtml(gr.name)}" style="background:${hexWithAlpha(gr.color, 0.16)};color:${gr.color};">
          <span style="width:7px;height:7px;border-radius:999px;background:${gr.color};"></span>
          <span>${escHtml(gr.name)}</span>
          <span style="font-weight:400;opacity:0.7;">${gr.count}</span>
        </div>
      </div>
    `;
  }

  // 2. Relações, portas e cardinalidade
  let svgPaths = '';

  const edges =
    scene?.edges ??
    schema.refs
      .map((r) => {
        const fromQ = qualifiedEndpointTable(r.from);
        const toQ = qualifiedEndpointTable(r.to);
        const ft = schema.tables.find((t) => qualifiedTableName(t) === fromQ);
        const tt = schema.tables.find((t) => qualifiedTableName(t) === toQ);
        const aPos = positions[fromQ];
        const bPos = positions[toQ];
        if (!ft || !tt || !aPos || !bPos) return null;
        const fc = ft.columns.findIndex((c) => c.name === r.from.column);
        const tc = tt.columns.findIndex((c) => c.name === r.to.column);
        if (fc < 0 || tc < 0) return null;
        const ay = aPos.y + HEADER_H + 4 + ROW_H * (fc + 0.5);
        const by = bPos.y + HEADER_H + 4 + ROW_H * (tc + 0.5);
        const [sa, sb] =
          aPos.x + CARD_W < bPos.x
            ? [1, -1]
            : bPos.x + CARD_W < aPos.x
            ? [-1, 1]
            : [1, 1];
        const ax = sa > 0 ? aPos.x + CARD_W : aPos.x;
        const bx = sb > 0 ? bPos.x + CARD_W : bPos.x;
        const mx = (ax + bx) / 2;
        const svg_path = `M ${ax} ${ay} L ${mx} ${ay} L ${mx} ${by} L ${bx} ${by}`;
        const [start_label, end_label] =
          r.kind === 'ManyToOne'
            ? ['N', '1']
            : r.kind === 'OneToMany'
            ? ['1', 'N']
            : r.kind === 'OneToOne'
            ? ['1', '1']
            : ['N', 'N'];
        return {
          from_table: fromQ,
          from_col: r.from.column,
          to_table: toQ,
          to_col: r.to.column,
          ax,
          ay,
          bx,
          by,
          sa,
          sb,
          points: [],
          svg_path,
          start_label,
          start_label_pos: { x: ax + sa * 10 - 4, y: ay - 15 },
          end_label,
          end_label_pos: { x: bx + sb * 10 - 4, y: by - 15 },
          cardinality: 'N:1',
        };
      })
      .filter(Boolean);

  for (const e of edges) {
    if (!e) continue;
    const lit = isSelected(e.from_table) || isSelected(e.to_table);
    const a = positions[e.from_table], b = positions[e.to_table];
    if (!a || !b) continue;
    const curve0 = curvedEdge(a.x, e.ay, b.x, e.by, edgeStyles?.[edgeKey(e)]);
    const curve = { ...curve0, path: edgeDrawing(e, curve0, edgeStyles?.[edgeKey(e)]).path };
    const stroke = lit ? g.accent : g.relStroke;
    if (lit) svgPaths += `<path d="${curve.path}" fill="none" stroke="${g.accent}" stroke-opacity="0.16" stroke-width="8" stroke-linecap="round" />`;
    svgPaths += `<path d="${curve.path}" fill="none" stroke="${stroke}" stroke-width="${lit ? 1.8 : 1.4}" />`;
    svgPaths += `<circle cx="${curve.x1}" cy="${e.ay}" r="3" fill="${stroke}" />`;
    svgPaths += `<circle cx="${curve.x2}" cy="${e.by}" r="3.5" fill="${g.card}" stroke="${stroke}" stroke-width="1.5" />`;
    const label = lit ? g.accent : g.textFaint;
    svgPaths += `<text x="${curve.x1 + Math.sign(curve.d1) * 11}" y="${e.ay - 6}" text-anchor="middle" font-family="Geist Mono, monospace" font-size="10" font-weight="500" fill="${label}">${e.start_label}</text>`;
    svgPaths += `<text x="${curve.x2 + Math.sign(curve.d2) * 11}" y="${e.by - 6}" text-anchor="middle" font-family="Geist Mono, monospace" font-size="10" font-weight="500" fill="${label}">${e.end_label}</text>`;
  }

  // Linha provisória ao arrastar relação (`Drag::Link`)
  if (linkDrag) {
    svgPaths += `<line x1="${linkDrag.startX}" y1="${linkDrag.startY}" x2="${linkDrag.nowX}" y2="${linkDrag.nowY}" stroke="${g.accent}" stroke-width="2" />`;
    svgPaths += `<circle cx="${linkDrag.startX}" cy="${linkDrag.startY}" r="3.5" fill="${g.accent}" />`;
    svgPaths += `<circle cx="${linkDrag.nowX}" cy="${linkDrag.nowY}" r="3.5" fill="${g.accent}" />`;
  }

  html += `<svg class="dbm-edges-svg" width="1" height="1">${svgPaths}</svg>`;

  // 3. Cartões de Tabela (`canvas.rs` linhas 859-1152)
  const fkSet = new Set(schema.refs.map((r) => `${qualifiedEndpointTable(r.from)}::${r.from.column}`));
  const orderedTables = [...schema.tables].sort((a, b) =>
    qualifiedTableName(a) === topTable ? 1 : qualifiedTableName(b) === topTable ? -1 : 0
  );

  for (const t of orderedTables) {
    const qname = qualifiedTableName(t);
    const pos = positions[qname] || { x: 0, y: 0 };
    const on = isSelected(qname);
    const visual = scene?.tables.find((tv) => tv.name === qname);
    const tint =
      visual?.tint ||
      t.color ||
      (() => {
        const gIdx = schema.groups.findIndex((gr) => gr.tables.includes(qname));
        return gIdx >= 0 ? TABLE_COLORS[gIdx % TABLE_COLORS.length][1] : null;
      })();

    let rowsHtml = '';
    t.columns.forEach((c) => {
      const isFk = fkSet.has(`${qname}::${c.name}`);
      const icon = c.pk
        ? iconSvg('KeyRound', 12 * g.uiScale, g.accent)
        : isFk
        ? iconSvg('Link2', 12 * g.uiScale, g.textMuted)
        : '';

      rowsHtml += `
        <div class="dbm-col-row" data-table="${escHtml(qname)}" data-col="${escHtml(c.name)}">
          <div style="width:14px;display:flex;justify-content:center;flex:none;">${icon}</div>
          <div style="flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:${g.text};font-weight:${c.pk ? 500 : 400};">${escHtml(c.name)}</div>
          <div style="font-size:${11.5 * g.uiScale}px;color:${g.textMuted};">${escHtml(c.ty)}</div>
          ${c.not_null ? `<div style="font-size:${10 * g.uiScale}px;color:${g.textMuted};">NN</div>` : ''}
          <div class="dbm-col-port" title="Drag to another column to create relation" data-port-table="${escHtml(qname)}" data-port-col="${escHtml(c.name)}"></div>
        </div>
      `;
    });

    const cardHeight = cardH(t.columns.length);
    const iconBg = tint ? hexWithAlpha(tint, 0.2) : g.selected;
    const iconFg = tint || g.accent;
    const hdrBg = tint ? hexWithAlpha(tint, 0.08) : 'transparent';
    const schemaBadge = t.schema
      ? `<span style="padding:1px 5px;border-radius:4px;background:${g.selected};color:${g.textMuted};font-size:${10 * g.uiScale}px;font-weight:500;flex:none;">${escHtml(t.schema)}</span>`
      : '';

    html += `
      <div class="dbm-card ${on ? 'selected' : ''}" data-card-table="${escHtml(qname)}" style="left:${pos.x}px;top:${pos.y}px;height:${cardHeight}px;cursor:${tool === 'select' ? 'move' : 'grab'};">
        ${tint ? `<div class="dbm-card-tint" style="background:${tint};"></div>` : ''}
        <div class="dbm-card-hdr" style="background:${hdrBg};">
          <div class="dbm-card-icon" style="background:${iconBg};">
            ${iconSvg('Table2', 13 * g.uiScale, iconFg)}
          </div>
          <div class="dbm-card-title">${escHtml(t.name)}</div>
          ${schemaBadge}
          <div class="dbm-card-count">${t.columns.length}</div>
        </div>
        <div class="dbm-card-rows">${rowsHtml}</div>
        <button class="dbm-card-addcol" title="Add column" data-add-col-table="${escHtml(qname)}">
          ${iconSvg('Plus', 12, g.textMuted)}
        </button>
      </div>
    `;
  }

  // 4. Retângulo de seleção Marquee (`canvas.rs` linhas 1153-1167)
  if (marqueeDrag) {
    const x0 = Math.min(marqueeDrag.startX, marqueeDrag.nowX);
    const y0 = Math.min(marqueeDrag.startY, marqueeDrag.nowY);
    const w = Math.abs(marqueeDrag.startX - marqueeDrag.nowX);
    const h = Math.abs(marqueeDrag.startY - marqueeDrag.nowY);
    html += `
      <div style="position:absolute;left:${x0}px;top:${y0}px;width:${w}px;height:${h}px;border:1px solid ${g.accent};background:${hexWithAlpha(g.accent, 0.08)};pointer-events:none;z-index:10;"></div>
    `;
  }

  return html;
}

/** Renderiza as 3 Ilhas Flutuantes do Canvas, Legenda e Empty State (`canvas.rs::overlays`) */
export function renderCanvasOverlays(params: {
  tool: 'select' | 'hand';
  zoom: number;
  legendOpen: boolean;
  editorOpen: boolean;
  exportMenuOpen: boolean;
  isEmpty: boolean;
  g: Glass;
}): string {
  const { tool, zoom, legendOpen, editorOpen, exportMenuOpen, isEmpty, g } = params;
  const pct = `${Math.round(zoom * 100)}%`;

  let html = `
    <!-- Ilha Topo-Centro: Ferramentas -->
    <div style="position:absolute;top:12px;left:0;width:100%;display:flex;justify-content:center;pointer-events:none;z-index:12;">
      <div class="dbm-island" style="pointer-events:auto;position:relative;">
        <button class="dbm-tool-btn ${tool === 'select' ? 'active' : ''}" title="Select (V)" data-action="ToolSelect">
          ${iconSvg('MousePointer2', 17)}
        </button>
        <button class="dbm-tool-btn ${tool === 'hand' ? 'active' : ''}" title="Hand: pan view (H)" data-action="ToolHand">
          ${iconSvg('Hand', 17)}
        </button>
        <div class="dbm-island-sep"></div>
        <button class="dbm-tool-btn" title="New Table (T)" data-action="NewTable">
          ${iconSvg('Plus', 17)}
        </button>
        <div class="dbm-island-sep"></div>
        <button class="dbm-tool-btn" title="Auto-Layout (Ctrl+L)" data-action="Relayout">
          ${iconSvg('LayoutGrid', 17)}
        </button>
        <button class="dbm-tool-btn" title="Fit to Screen (Shift+1)" data-action="FitView">
          ${iconSvg('Scan', 17)}
        </button>
        <div class="dbm-island-sep"></div>
        <button class="dbm-tool-btn ${editorOpen ? 'active' : ''}" title="DBML Editor (Ctrl+E)" data-action="ToggleEditor">
          ${iconSvg('Braces', 17)}
        </button>
        <button class="dbm-tool-btn ${exportMenuOpen ? 'active' : ''}" title="Export diagram (SVG, PNG, JPEG, Mermaid)" data-toggle-export="true">
          ${iconSvg('Download', 17)}
        </button>
        ${
          exportMenuOpen
            ? `
          <div style="position:absolute;top:44px;right:0;width:190px;padding:6px;border-radius:8px;border:1px solid ${g.edge};background:${g.sidebar};box-shadow:${g.islandShadow};display:flex;flex-direction:column;gap:2px;z-index:50;">
            <div style="padding:4px 8px;font-size:10.5px;font-weight:600;color:${g.textFaint};">EXPORT DIAGRAM</div>
            <button class="dbm-row-btn" data-action="ExportSvg" style="margin:0;">Vector SVG (.svg)</button>
            <button class="dbm-row-btn" data-action="ExportPng" style="margin:0;">PNG Image (.png)</button>
            <button class="dbm-row-btn" data-action="ExportJpeg" style="margin:0;">JPEG Image (.jpg)</button>
            <button class="dbm-row-btn" data-action="ExportMermaid" style="margin:0;">Mermaid erDiagram (.mmd)</button>
          </div>
        `
            : ''
        }
      </div>
    </div>

    <!-- Ilha Base-Esquerda: Zoom -->
    <div style="position:absolute;bottom:12px;left:12px;z-index:12;">
      <div class="dbm-island">
        <button class="dbm-tool-btn" title="Zoom Out (Ctrl+-)" data-action="ZoomOut">
          ${iconSvg('ZoomOut', 17)}
        </button>
        <button title="Fit to Screen (Shift+1)" data-action="FitView" style="height:28px;padding:0 8px;border-radius:6px;border:none;background:transparent;color:${g.text};font-size:12.5px;font-weight:500;cursor:pointer;">
          ${pct}
        </button>
        <button class="dbm-tool-btn" title="Zoom In (Ctrl+=)" data-action="ZoomIn">
          ${iconSvg('ZoomIn', 17)}
        </button>
      </div>
    </div>

    <!-- Ilha Base-Direita: Ajuda / Legenda -->
    <div style="position:absolute;bottom:12px;right:12px;z-index:12;">
      <div class="dbm-island">
        <button class="dbm-tool-btn ${legendOpen ? 'active' : ''}" title="Legend (Ctrl+Shift+L)" data-action="ToggleLegend">
          ${iconSvg('Info', 17)}
        </button>
      </div>
    </div>
  `;

  if (legendOpen) {
    html += `
      <div style="position:absolute;bottom:52px;right:12px;max-width:280px;z-index:12;">
        <div class="dbm-island" style="flex-direction:column;align-items:flex-start;gap:6px;padding:10px 12px;">
          <div style="width:100%;display:flex;justify-content:space-between;align-items:center;">
            <span style="font-size:12px;font-weight:600;color:${g.text};">Legend</span>
            <button class="dbm-rail-btn" style="width:20px;height:20px;" title="Hide legend" data-action="ToggleLegend">
              ${iconSvg('X', 13, g.textMuted)}
            </button>
          </div>
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;color:${g.textMuted};">
            <span style="width:22px;text-align:center;font-weight:600;color:${g.accent};">PK</span>
            <span>Primary key</span>
          </div>
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;color:${g.textMuted};">
            <span style="width:22px;display:flex;justify-content:center;"><span style="width:20px;height:1.5px;background:${g.textMuted};display:block;"></span></span>
            <span>Relation (column → column)</span>
          </div>
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;color:${g.textMuted};">
            <span style="width:22px;text-align:center;font-size:11px;">N · 1</span>
            <span>Cardinality: many · one</span>
          </div>
          <div style="display:flex;align-items:center;gap:8px;font-size:12px;color:${g.textMuted};">
            <span style="width:22px;display:flex;justify-content:center;"><span style="width:20px;height:2px;background:${g.accent};display:block;"></span></span>
            <span>Selected table and its relations</span>
          </div>
        </div>
      </div>
    `;
  }

  if (isEmpty) {
    html += `
      <div style="position:absolute;inset:0;display:flex;align-items:center;justify-content:center;pointer-events:none;z-index:8;">
        <div style="pointer-events:auto;max-width:360px;padding:24px;border-radius:12px;border:1px solid ${g.edge};background:${g.card};box-shadow:${g.cardShadow};display:flex;flex-direction:column;align-items:center;text-align:center;gap:10px;">
          <div style="width:40px;height:40px;border-radius:10px;background:${g.hover};display:flex;align-items:center;justify-content:center;">
            ${iconSvg('Database', 20, g.accent)}
          </div>
          <div style="font-size:15px;font-weight:600;color:${g.text};">No tables yet</div>
          <div style="font-size:13px;color:${g.textMuted};line-height:1.45;">Write your DBML schema in the editor or create a new table on the canvas.</div>
          <div style="display:flex;gap:8px;margin-top:6px;">
            <button data-action="ToggleEditor" style="height:30px;padding:0 10px;border-radius:6px;border:1px solid ${g.edge};background:${g.hover};color:${g.text};font-size:12.5px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
              ${iconSvg('Braces', 14, g.textMuted)}
              <span>Open DBML Editor</span>
              ${renderKbd(['Ctrl', 'E'])}
            </button>
            <button data-action="NewTable" style="height:30px;padding:0 10px;border-radius:6px;border:1px solid ${g.edge};background:${g.hover};color:${g.text};font-size:12.5px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
              ${iconSvg('Plus', 14, g.textMuted)}
              <span>New Table</span>
              ${renderKbd(['T'])}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  return html;
}
