import { t } from './i18n';
/**
 * Módulo de Exportação do DB Modeling Canvas.
 * Suporta exportação para PNG, JPEG, SVG vetorial (com grupos e polilinhas ortogonais)
 * e sintaxe Mermaid erDiagram.
 */

import { Schema, RefKind, SceneGeometry, EdgeStyle, RoutedEdge, edgeKey, qualifiedEndpointTable, qualifiedTableName } from './types';
import { Point } from './viewport';
import { CARD_W, HEADER_H, ROW_H, cardH, getGlass, hexWithAlpha, Mode } from './glass';
import { iconSvg, IconName } from './icons';
import { curvedEdge, edgeDrawing } from './edge-path';
import { FONT_FACE_CSS } from './fonts';
import { canvasGridStep, canvasGroups } from './scene-style';

function mermaidCardinality(kind: RefKind): string {
  switch (kind) {
    case 'ManyToOne':
      return '}o--||';
    case 'OneToMany':
      return '||--o{';
    case 'OneToOne':
      return '||--||';
    case 'ManyToMany':
      return '}o--o{';
  }
}

export function exportToMermaid(schema: Schema): string {
  const lines: string[] = ['erDiagram'];

  for (const table of schema.tables) {
    const tableName = table.name.replace(/[^a-zA-Z0-9_]/g, '_');
    lines.push(`    ${tableName} {`);

    for (const col of table.columns) {
      const colType = (col.ty || 'string').replace(/[^a-zA-Z0-9_]/g, '_');
      const colName = col.name.replace(/[^a-zA-Z0-9_]/g, '_');
      const keys: string[] = [];
      if (col.pk) keys.push('PK');
      if (col.unique && !col.pk) keys.push('UK');

      const keySuffix = keys.length > 0 ? ` ${keys.join(',')}` : '';
      lines.push(`        ${colType} ${colName}${keySuffix}`);
    }

    lines.push('    }');
  }

  for (const ref of schema.refs) {
    const fromTable = qualifiedEndpointTable(ref.from).replace(/[^a-zA-Z0-9_]/g, '_');
    const toTable = qualifiedEndpointTable(ref.to).replace(/[^a-zA-Z0-9_]/g, '_');
    const symbol = mermaidCardinality(ref.kind);
    const label = `"${ref.from.column} to ${ref.to.column}"`;
    lines.push(`    ${fromTable} ${symbol} ${toTable} : ${label}`);
  }

  return lines.join('\n');
}

/** Ícone Lucide aninhado no SVG exportado, posicionado em (x, y). */
function svgIcon(name: IconName, x: number, y: number, size: number, color: string): string {
  return iconSvg(name, size, escapeXml(color)).replace('<svg ', `<svg x="${x}" y="${y}" `);
}

function escapeXml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

interface CanvasAppearance {
  zoom?: number;
  selection?: readonly string[];
  selectedRef?: string;
  selectedEnum?: string | null;
  top?: string | null;
}

const F = 'Geist, system-ui, sans-serif';
const M = "'Geist Mono', ui-monospace, monospace";

/** Exporta o diagrama completo, nas coordenadas e com a aparência do canvas. */
export function exportToSvg(
  schema: Schema,
  positions: Record<string, Point>,
  scene?: SceneGeometry | null,
  mode: Mode = 'dark',
  edges: Record<string, EdgeStyle> = {},
  appearance: CanvasAppearance = {}
): string {
  const g = getGlass(mode, 1);
  const dark = mode === 'dark';
  const sel = new Set(appearance.selection);
  const zoom = appearance.zoom && Number.isFinite(appearance.zoom) && appearance.zoom > 0 ? appearance.zoom : 1;
  const step = canvasGridStep(zoom) / zoom;
  const ctx = typeof document === 'undefined' ? null : document.createElement('canvas').getContext('2d');
  const measure = (text: string, size: number, family = M, weight = 400) => {
    if (!ctx) return Array.from(text).length * size * 0.6;
    ctx.font = `${weight} ${size}px ${family}`;
    return ctx.measureText(text).width;
  };
  const fitText = (text: string, available: number, size: number, family: string, weight: number) => {
    if (measure(text, size, family, weight) <= available) return text;
    if (measure('…', size, family, weight) > available) return '';
    const chars = Array.from(text);
    let low = 0, high = chars.length;
    while (low < high) {
      const mid = Math.ceil((low + high) / 2);
      if (measure(chars.slice(0, mid).join('') + '…', size, family, weight) <= available) low = mid;
      else high = mid - 1;
    }
    return chars.slice(0, low).join('') + '…';
  };
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  const include = (x: number, y: number, w = 0, h = 0) => {
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x + w);
    maxY = Math.max(maxY, y + h);
  };
  const text = (x: number, y: number, value: string | number, cls: string, attrs = '') =>
    `<text x="${x}" y="${y}" class="${cls}" ${attrs}>${escapeXml(String(value))}</text>`;
  const card = (w: number, h: number, selected: boolean, outline = g.edgeSoft) => {
    const ring = selected ? `<rect x="-2" y="-2" width="${w + 4}" height="${h + 4}" rx="14" fill="none" stroke="${g.accent}" stroke-opacity=".18" stroke-width="2"/>` : '';
    return `${ring}<rect x=".5" y=".5" width="${w - 1}" height="${h - 1}" rx="12" class="table-box" stroke="${selected ? g.accent : outline}"/>`;
  };
  const header = (w: number, h: number, icon: IconName, tint: string) =>
    `<path d="M1 ${h + .5}H${w - 1}" stroke="${g.edgeSoft}"/>${svgIcon(icon, 13, 1 + (h - 14) / 2, 14, tint)}`;

  const parts: string[] = [];
  const tables = new Map(schema.tables.map(table => [qualifiedTableName(table), table]));
  const visuals = new Map(scene?.tables.map(visual => [visual.name, visual]));
  const fromColumnY = (table: string, column: string, y: number) =>
    y + HEADER_H + 4 + (Math.max(0, tables.get(table)?.columns.findIndex(c => c.name === column) ?? 0) + .5) * ROW_H;
  const routed: Array<Pick<RoutedEdge, 'from_table' | 'to_table' | 'from_col' | 'to_col' |
    'ay' | 'by' | 'auto_path' | 'auto_mid' | 'svg_path' | 'start_label' | 'end_label'>> = scene
    ? scene.edges
    : schema.refs.flatMap(ref => {
      const from = qualifiedEndpointTable(ref.from), to = qualifiedEndpointTable(ref.to);
      const a = positions[from], b = positions[to];
      if (!a || !b || !tables.has(from) || !tables.has(to)) return [];
      return [{
        from_table: from, to_table: to, from_col: ref.from.column, to_col: ref.to.column,
        ay: fromColumnY(from, ref.from.column, a.y), by: fromColumnY(to, ref.to.column, b.y),
        svg_path: '',
        start_label: ref.kind === 'OneToMany' || ref.kind === 'OneToOne' ? '1' : 'N',
        end_label: ref.kind === 'ManyToOne' || ref.kind === 'OneToOne' ? '1' : 'N',
      }];
    });

  for (const edge of routed) {
    const a = positions[edge.from_table], b = positions[edge.to_table];
    if (!a || !b) continue;
    const key = edgeKey(edge);
    const style = edges[key];
    const curve = curvedEdge(a.x, edge.ay, b.x, edge.by, style);
    const d = escapeXml(edgeDrawing(edge, curve, style).path);
    const controls = !style && edge.auto_path ? edge.auto_path.match(/-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gi) : null;
    if (controls) {
      for (let i = 0; i + 1 < controls.length; i += 2) include(Number(controls[i]), Number(controls[i + 1]));
    } else {
      include(curve.c1.x, curve.c1.y);
      include(curve.c2.x, curve.c2.y);
    }
    const lit = appearance.selectedRef === key || sel.has(edge.from_table) || sel.has(edge.to_table);
    const color = lit ? g.accent : g.relStroke;
    if (lit) parts.push(`<path d="${d}" fill="none" stroke="${color}" stroke-opacity=".16" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>`);
    parts.push(`<path d="${d}" class="ref-edge" stroke="${color}" stroke-width="${lit ? 1.8 : 1.4}"/>`,
      `<circle cx="${curve.x1}" cy="${edge.ay}" r="3" fill="${color}"/>`,
      `<circle cx="${curve.x2}" cy="${edge.by}" r="3.5" fill="${g.card}" stroke="${color}" stroke-width="1.5"/>`);
    const label = `text-anchor="middle" fill="${lit ? g.accent : g.textFaint}"`;
    parts.push(text(curve.x1 + Math.sign(curve.d1) * 11, edge.ay - 6, edge.start_label, 'ref-label', label),
      text(curve.x2 + Math.sign(curve.d2) * 11, edge.by - 6, edge.end_label, 'ref-label', label));
  }

  for (const group of canvasGroups(scene?.groups ?? [], schema, positions)) {
    const { x, y, w, h, name, count, color } = group;
    include(x, y, w, h);
    const fg = escapeXml(color);
    const pillX = x + 44 + measure(name, 12, F, 600);
    const pillW = measure(String(count), 10.5, M, 500) + 12;
    parts.push(`<g><rect x="${x + .5}" y="${y + .5}" width="${w - 1}" height="${h - 1}" rx="20" fill="${escapeXml(hexWithAlpha(color, dark ? .06 : .07))}" stroke="${escapeXml(hexWithAlpha(color, dark ? .18 : .22))}"/>`,
      svgIcon('Layers', x + 17, y + 14.5, 13, color),
      text(x + 37, y + 21, name, 'group-label middle', `fill="${fg}"`),
      `<rect x="${pillX}" y="${y + 13}" width="${pillW}" height="16" rx="8" fill="${escapeXml(hexWithAlpha(color, .14))}"/>`,
      text(pillX + pillW / 2, y + 21, count, 'group-count middle', `fill="${fg}" text-anchor="middle"`), '</g>');
  }

  const order = [...(scene?.tables.map(tv => tv.name) ?? tables.keys())]
    .sort((a, b) => Number(a === appearance.top) - Number(b === appearance.top));
  const fkColumns = new Set(routed.map(edge => `${edge.from_table}.${edge.from_col}`));
  for (const key of order) {
    const table = tables.get(key), p = positions[key];
    if (!table || !p) continue;
    const visual = visuals.get(key);
    const h = cardH(table.columns.length);
    include(p.x, p.y, CARD_W, h);
    parts.push(`<g transform="translate(${p.x} ${p.y})">`, card(CARD_W, h, sel.has(key)),
      header(CARD_W, HEADER_H, 'Table2', visual?.tint || (!scene && table.color) || g.accent));
    const countX = CARD_W - 13;
    let titleRight = countX - measure(String(table.columns.length), 10.5) - 9;
    if (table.schema) {
      const badgeW = measure(table.schema, 10, M, 500) + 10;
      parts.push(`<rect x="${titleRight - badgeW}" y="15" width="${badgeW}" height="16" rx="4" fill="${g.input}"/>`,
        text(titleRight - badgeW / 2, 23, table.schema, 'schema middle', 'text-anchor="middle"'));
      titleRight -= badgeW + 9;
    }
    parts.push(text(36, 23, fitText(table.name, titleRight - 36, 13, F, 600), 'header-title middle'),
      text(countX, 23, table.columns.length, 'count middle', 'text-anchor="end"'));
    const indexed = new Set(table.indexes?.flatMap(ix => ix.columns.filter(ic => ic.kind === 'column').map(ic => ic.value)));
    table.columns.forEach((col, i) => {
      const y = 1 + HEADER_H + 4 + (i + .5) * ROW_H;
      const fk = col.fk || (visual ? visual.fk_columns.includes(col.name) : fkColumns.has(`${key}.${col.name}`));
      const icon = col.pk ? 'KeyRound' : fk ? 'Link' : 'Dot';
      parts.push(svgIcon(icon, 14, y - 6, 12, col.pk ? g.accent : fk ? g.link : g.textFaint));
      let right = CARD_W - 13;
      parts.push(text(right, y, col.ty, 'col-type middle', 'text-anchor="end"'));
      right -= measure(col.ty, 11) + 8;
      const badges = [col.unique && 'UQ', col.not_null && 'NN', indexed.has(col.name) && 'IDX'];
      for (const badge of badges) {
        if (!badge) continue;
        parts.push(text(right, y, badge, 'column-badge middle', `text-anchor="end" fill="${badge === 'IDX' ? g.idx : g.textFaint}"`));
        right -= measure(badge, 9, M, 600) + 8;
      }
      const name = fitText(col.name, right - 35, 12.5, F, col.pk ? 500 : 400);
      parts.push(text(35, y, name, col.pk ? 'col-name pk middle' : 'col-name middle'));
    });
    parts.push('</g>');
  }

  for (const en of scene?.enums ?? []) {
    const p = positions[`enum:${en.name}`] || en;
    const h = 52 + Math.max(1, en.values.length) * 26;
    const selected = appearance.selectedEnum === en.name;
    const outline = en.referenced_by.some(name => sel.has(name)) ? g.accent : g.edge;
    include(p.x, p.y, en.w, h);
    parts.push(`<g transform="translate(${p.x} ${p.y})">`, card(en.w, h, selected, outline),
      header(en.w, 40, 'List', g.accent),
      text(36, 21, fitText(en.name, en.w - 80, 13, F, 600), 'header-title middle'),
      text(en.w - 13, 21, 'ENUM', 'column-badge middle', `text-anchor="end" fill="${g.textFaint}"`));
    en.values.forEach((value, i) => {
      const y = 58 + i * 26;
      parts.push(svgIcon('Dot', 14, y - 6, 12, g.accent),
        text(36, y, value, 'enum-value middle'));
    });
    if (!en.values.length) parts.push(text(13, 58, t('(vazio)'), 'enum-value middle', 'opacity=".7"'));
    parts.push('</g>');
  }

  if (!Number.isFinite(minX)) include(0, 0, 800, 600);
  const padding = 40;
  const x = Math.floor(minX - padding), y = Math.floor(minY - padding);
  const width = Math.ceil(maxX + padding - x), height = Math.ceil(maxY + padding - y);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${width} ${height}" width="${width}" height="${height}">
<defs>
<style>
${FONT_FACE_CSS}
.middle { dominant-baseline: central; }
.table-box { fill: ${g.card}; filter: url(#card-shadow); }
.header-title,.col-name { fill: ${g.text}; font-family: ${F}; }
.header-title { font-size: 13px; font-weight: 600; }
.col-name { font-size: 12.5px; }
.col-name.pk { font-weight: 500; }
.col-type,.count,.schema,.enum-value,.column-badge,.group-count,.ref-label { font-family: ${M}; }
.col-type,.count { fill: ${g.textFaint}; }
.col-type { font-size: 11px; }
.count,.group-count { font-size: 10.5px; }
.schema,.ref-label { font-weight: 500; font-size: 10px; }
.schema,.enum-value { fill: ${g.textMuted}; }
.enum-value { font-size: 12px; }
.column-badge { font-size: 9px; font-weight: 600; letter-spacing: .02em; }
.ref-edge { fill: none; stroke-linejoin: round; }
.group-label { font: 600 12px ${F}; }
.group-count { font-weight: 500; }
</style>
<pattern id="dot-grid" width="${step}" height="${step}" patternUnits="userSpaceOnUse"><circle cx="${step / 2}" cy="${step / 2}" r="${1 / zoom}" fill="${g.grid}"/></pattern>
<filter id="card-shadow" x="-25%" y="-40%" width="150%" height="180%"><feDropShadow dx="0" dy="${dark ? 10 : 6}" stdDeviation="${dark ? 15 : 9}" flood-opacity="${dark ? .35 : .05}"/></filter>
</defs>
<rect x="${x}" y="${y}" width="${width}" height="${height}" fill="${g.surface}"/>
<rect x="${x}" y="${y}" width="${width}" height="${height}" fill="url(#dot-grid)"/>
${parts.join('\n')}
</svg>`;
}

/**
 * Converte uma string SVG vetorial em Blob PNG ou JPEG rasterizando em alta resolução (2x Retina).
 */
export function exportSvgStringToRasterBlob(
  svgString: string,
  format: 'image/png' | 'image/jpeg' = 'image/png',
  quality = 0.95
): Promise<Blob> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    const svgBlob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(svgBlob);

    img.onload = () => {
      const scale = 2;
      const canvas = document.createElement('canvas');
      canvas.width = (img.width || 1200) * scale;
      canvas.height = (img.height || 800) * scale;
      const ctx = canvas.getContext('2d');
      if (!ctx) {
        URL.revokeObjectURL(url);
        reject(new Error(t('Não foi possível criar contexto 2D para rasterização.')));
        return;
      }
      ctx.scale(scale, scale);
      ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);
      canvas.toBlob(
        (blob) => {
          if (blob) resolve(blob);
          else reject(new Error('Falha ao gerar Blob de imagem.'));
        },
        format,
        quality
      );
    };

    img.onerror = (err) => {
      URL.revokeObjectURL(url);
      reject(err);
    };

    img.src = url;
  });
}

export function exportCanvasToBlob(
  canvas: HTMLCanvasElement,
  format: 'image/png' | 'image/jpeg' = 'image/png',
  quality = 0.95
): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (blob) {
          resolve(blob);
        } else {
          reject(new Error('Falha ao gerar Blob a partir do canvas.'));
        }
      },
      format,
      quality
    );
  });
}
