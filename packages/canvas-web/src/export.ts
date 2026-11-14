/**
 * Módulo de Exportação do DB Modeling Canvas.
 * Suporta exportação para PNG, JPEG, SVG vetorial (com grupos e polilinhas ortogonais)
 * e sintaxe Mermaid erDiagram.
 */

import { Schema, RefKind, SceneGeometry, qualifiedEndpointTable, qualifiedTableName } from './types';
import { Point } from './viewport';
import { CARD_W, HEADER_H, ROW_H, cardH, getGlass, Mode, TABLE_COLORS } from './glass';

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

function escapeXml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

/**
 * Exporta a cena como uma string SVG vetorial independente, reproduzindo fielmente
 * o estilo `Glass`, grupos, arestas ortogonais e cartões de tabela.
 */
export function exportToSvg(
  schema: Schema,
  positions: Record<string, Point>,
  scene?: SceneGeometry | null,
  mode: Mode = 'dark'
): string {
  const g = getGlass(mode, 1.0);
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const table of schema.tables) {
    const pos = positions[table.name];
    if (!pos) continue;
    const tableH = cardH(table.columns.length);
    minX = Math.min(minX, pos.x);
    minY = Math.min(minY, pos.y);
    maxX = Math.max(maxX, pos.x + CARD_W);
    maxY = Math.max(maxY, pos.y + tableH);
  }

  if (scene?.groups) {
    for (const grp of scene.groups) {
      minX = Math.min(minX, grp.x);
      minY = Math.min(minY, grp.y);
      maxX = Math.max(maxX, grp.x + grp.w);
      maxY = Math.max(maxY, grp.y + grp.h);
    }
  }

  if (minX === Infinity) {
    minX = 0;
    minY = 0;
    maxX = 800;
    maxY = 600;
  }

  const padding = 48;
  const viewBoxX = Math.floor(minX - padding);
  const viewBoxY = Math.floor(minY - padding);
  const width = Math.ceil(maxX - minX + padding * 2);
  const height = Math.ceil(maxY - minY + padding * 2);

  const svgParts: string[] = [];
  svgParts.push(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBoxX} ${viewBoxY} ${width} ${height}" width="${width}" height="${height}">`
  );
  svgParts.push(`  <defs>
    <style>
      .bg { fill: ${g.surface}; }
      .table-box { fill: ${g.card}; stroke: ${g.edge}; stroke-width: 1; rx: 12; ry: 12; }
      .header-box { fill: transparent; }
      .header-title { fill: ${g.text}; font-family: Inter, system-ui, sans-serif; font-size: 14px; font-weight: 600; }
      .col-name { fill: ${g.text}; font-family: Inter, system-ui, sans-serif; font-size: 13px; }
      .col-name-pk { fill: ${g.text}; font-family: Inter, system-ui, sans-serif; font-size: 13px; font-weight: 500; }
      .col-type { fill: ${g.textMuted}; font-family: Inter, system-ui, sans-serif; font-size: 11.5px; }
      .pk-chip { fill: ${g.accent}; font-family: Inter, system-ui, sans-serif; font-size: 10px; font-weight: 600; }
      .nn-chip { fill: ${g.textMuted}; font-family: Inter, system-ui, sans-serif; font-size: 10px; }
      .ref-edge { stroke: ${g.textMuted}; stroke-opacity: 0.55; stroke-width: 1.5; fill: none; }
      .ref-label { fill: ${g.textMuted}; font-family: Inter, system-ui, sans-serif; font-size: 10px; }
      .group-box { fill-opacity: 0.06; stroke-opacity: 0.4; stroke-width: 1; rx: 14; ry: 14; }
      .group-label { font-family: Inter, system-ui, sans-serif; font-size: 12px; font-weight: 600; }
    </style>
  </defs>`);

  svgParts.push(
    `  <rect x="${viewBoxX}" y="${viewBoxY}" width="${width}" height="${height}" class="bg" />`
  );

  // 1. Molduras de grupos
  if (scene?.groups) {
    for (const grp of scene.groups) {
      svgParts.push(
        `  <g>` +
          `<rect x="${grp.x}" y="${grp.y}" width="${grp.w}" height="${grp.h}" fill="${grp.color}" stroke="${grp.color}" class="group-box" />` +
          `<rect x="${grp.x + 10}" y="${grp.y + 8}" width="${grp.name.length * 8 + 38}" height="22" rx="6" fill="${grp.color}" fill-opacity="0.16" />` +
          `<circle cx="${grp.x + 19}" cy="${grp.y + 19}" r="3.5" fill="${grp.color}" />` +
          `<text x="${grp.x + 28}" y="${grp.y + 23}" fill="${grp.color}" class="group-label">${escapeXml(grp.name)} (${grp.count})</text>` +
          `</g>`
      );
    }
  }

  // 2. Arestas (polilinhas ortogonais do Wasm quando disponíveis, ou fallback)
  if (scene?.edges && scene.edges.length > 0) {
    for (const edge of scene.edges) {
      svgParts.push(`  <path d="${edge.svg_path}" class="ref-edge" />`);
      svgParts.push(
        `  <circle cx="${edge.ax}" cy="${edge.ay}" r="3" fill="${g.textMuted}" fill-opacity="0.55" />`
      );
      svgParts.push(
        `  <circle cx="${edge.bx}" cy="${edge.by}" r="3" fill="${g.textMuted}" fill-opacity="0.55" />`
      );
      svgParts.push(
        `  <text x="${edge.start_label_pos.x}" y="${edge.start_label_pos.y + 10}" class="ref-label">${edge.start_label}</text>`
      );
      svgParts.push(
        `  <text x="${edge.end_label_pos.x}" y="${edge.end_label_pos.y + 10}" class="ref-label">${edge.end_label}</text>`
      );
    }
  } else {
    for (const ref of schema.refs) {
      const fromQ = qualifiedEndpointTable(ref.from);
      const toQ = qualifiedEndpointTable(ref.to);
      const fromPos = positions[fromQ];
      const toPos = positions[toQ];
      if (!fromPos || !toPos) continue;

      const fromTable = schema.tables.find((t) => qualifiedTableName(t) === fromQ);
      const toTable = schema.tables.find((t) => qualifiedTableName(t) === toQ);
      if (!fromTable || !toTable) continue;

      const fromIdx = fromTable.columns.findIndex((c) => c.name === ref.from.column);
      const toIdx = toTable.columns.findIndex((c) => c.name === ref.to.column);

      const fromY =
        fromPos.y + HEADER_H + 4 + (fromIdx >= 0 ? fromIdx + 0.5 : 0.5) * ROW_H;
      const toY = toPos.y + HEADER_H + 4 + (toIdx >= 0 ? toIdx + 0.5 : 0.5) * ROW_H;

      const fromRight = fromPos.x + CARD_W < toPos.x;
      const startX = fromRight ? fromPos.x + CARD_W : fromPos.x;
      const endX = fromRight ? toPos.x : toPos.x + CARD_W;
      const midX = (startX + endX) / 2;

      const d = `M ${startX} ${fromY} L ${midX} ${fromY} L ${midX} ${toY} L ${endX} ${toY}`;
      svgParts.push(`  <path d="${d}" class="ref-edge" />`);
    }
  }

  // 3. Cartões de Tabelas
  for (const table of schema.tables) {
    const pos = positions[table.name];
    if (!pos) continue;

    const tableH = cardH(table.columns.length);
    const visual = scene?.tables.find((tv) => tv.name === table.name);
    const tint =
      visual?.tint ||
      table.color ||
      (() => {
        const grpIdx = schema.groups.findIndex((gr) => gr.tables.includes(table.name));
        return grpIdx >= 0 ? TABLE_COLORS[grpIdx % TABLE_COLORS.length][1] : null;
      })();

    svgParts.push(`  <g transform="translate(${pos.x}, ${pos.y})">`);
    svgParts.push(`    <rect width="${CARD_W}" height="${tableH}" class="table-box" />`);
    if (tint) {
      svgParts.push(
        `    <rect width="${CARD_W}" height="${HEADER_H}" rx="12" fill="${tint}" fill-opacity="0.08" />`
      );
      svgParts.push(
        `    <rect width="${CARD_W}" height="3" rx="2" fill="${tint}" />`
      );
    }
    svgParts.push(
      `    <line x1="0" y1="${HEADER_H}" x2="${CARD_W}" y2="${HEADER_H}" stroke="${g.edgeSoft}" />`
    );
    svgParts.push(
      `    <text x="42" y="27" class="header-title">${escapeXml(table.name)}</text>`
    );
    svgParts.push(
      `    <text x="${CARD_W - 16}" y="26" text-anchor="end" class="col-type">${table.columns.length}</text>`
    );

    table.columns.forEach((col, idx) => {
      const colY = HEADER_H + 4 + idx * ROW_H + 19;
      if (col.pk) {
        svgParts.push(`    <text x="12" y="${colY}" class="pk-chip">PK</text>`);
      }
      svgParts.push(
        `    <text x="34" y="${colY}" class="${col.pk ? 'col-name-pk' : 'col-name'}">${escapeXml(col.name)}</text>`
      );
      const typeX = col.not_null ? CARD_W - 34 : CARD_W - 12;
      svgParts.push(
        `    <text x="${typeX}" y="${colY}" text-anchor="end" class="col-type">${escapeXml(col.ty)}</text>`
      );
      if (col.not_null) {
        svgParts.push(
          `    <text x="${CARD_W - 12}" y="${colY}" text-anchor="end" class="nn-chip">NN</text>`
        );
      }
    });

    svgParts.push(`  </g>`);
  }

  svgParts.push(`</svg>`);
  return svgParts.join('\n');
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
        reject(new Error('Não foi possível criar contexto 2D para rasterização.'));
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
