/**
 * Renderizadores dos painéis laterais (Sidebar Esquema/Problemas, Inspector Completo,
 * Paleta de Comandos `Ctrl+K`, Modal de Atalhos `Ctrl+/` e Menu de Aparência).
 * Portado 1:1 de `crates/app/src/shell/panels.rs`.
 */

import { Schema, Diagnostic } from './types';
import { Glass, TABLE_COLORS, STANDARD_TYPES, hexWithAlpha } from './glass';
import { iconSvg } from './icons';
import { catalog, ActionId } from './actions';

export type EditTarget =
  | { kind: 'table'; table: string }
  | { kind: 'colName'; table: string; col: string }
  | { kind: 'colType'; table: string; col: string };

export function isSameTarget(a: EditTarget | null, b: EditTarget): boolean {
  if (!a || a.kind !== b.kind || a.table !== b.table) return false;
  if (a.kind === 'table') return true;
  return (a as { col: string }).col === (b as { col: string }).col;
}

export function escHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function renderKbd(keys: string[]): string {
  return keys.map((k) => `<span class="dbm-kbd">${escHtml(k)}</span>`).join(' ');
}

/** Renderiza a aba Esquema da Sidebar esquerda (`shell/panels.rs::schema_pane`) */
export function renderSchemaPane(
  schema: Schema,
  selection: string[],
  g: Glass
): string {
  const renderTableRow = (tableName: string, indent = false): string => {
    const t = schema.tables.find((x) => x.name === tableName);
    if (!t) return '';
    const on = selection.includes(t.name);
    const iconColor = t.color || (on ? g.accent : g.textFaint);
    return `
      <button class="dbm-row-btn ${on ? 'selected' : ''}" data-focus-table="${escHtml(t.name)}" style="${indent ? 'padding-left:22px;' : ''}">
        ${iconSvg('Table2', 14, iconColor)}
        <span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escHtml(t.name)}</span>
        <span style="font-size:11.5px;color:${g.textFaint};">${t.columns.length}</span>
      </button>
    `;
  };

  let body = `<div class="dbm-section-hdr"><span>TABLES</span><span>${schema.tables.length}</span></div>`;
  body += `<div style="display:flex;flex-direction:column;gap:1px;">`;

  schema.groups.forEach((gr, idx) => {
    const color = gr.color || TABLE_COLORS[idx % TABLE_COLORS.length][1];
    body += `
      <div style="display:flex;align-items:center;gap:6px;padding:8px 12px 2px 12px;font-size:12px;font-weight:600;color:${g.textMuted};">
        <span style="width:7px;height:7px;border-radius:999px;background:${color};"></span>
        <span style="flex:1;">${escHtml(gr.name)}</span>
        <span style="font-weight:400;color:${g.textFaint};">${gr.tables.length}</span>
      </div>
    `;
    for (const tblName of gr.tables) {
      body += renderTableRow(tblName, true);
    }
  });

  const groupedSet = new Set(schema.groups.flatMap((gr) => gr.tables));
  const loose = schema.tables.filter((t) => !groupedSet.has(t.name));
  if (schema.groups.length > 0 && loose.length > 0) {
    body += `<div style="padding:8px 12px 2px 12px;font-size:12px;font-weight:600;color:${g.textFaint};">No group</div>`;
  }
  for (const t of loose) {
    body += renderTableRow(t.name, false);
  }
  body += `</div>`;

  body += `<div class="dbm-section-hdr"><span>RELATIONS</span><span>${schema.refs.length}</span></div>`;
  body += `<div style="display:flex;flex-direction:column;gap:2px;padding:0 12px 12px 12px;">`;
  for (const r of schema.refs) {
    body += `
      <div style="display:flex;align-items:center;gap:6px;height:22px;font-size:12px;color:${g.textMuted};">
        ${iconSvg('Link2', 13, g.textFaint)}
        <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escHtml(r.from.table)} → ${escHtml(r.to.table)}</span>
      </div>
    `;
  }
  body += `</div>`;
  return body;
}

/** Renderiza a aba Problemas da Sidebar esquerda (`shell/panels.rs::problems_pane`) */
export function renderProblemsPane(
  diags: Diagnostic[],
  docTitle: string,
  g: Glass
): string {
  let html = `<div class="dbm-section-hdr"><span>PROBLEMS</span><span>${diags.length}</span></div>`;
  if (diags.length === 0) {
    html += `
      <div style="padding:24px 16px;display:flex;flex-direction:column;align-items:center;gap:8px;color:${g.textMuted};font-size:13px;">
        ${iconSvg('CircleCheck', 20, g.ok)}
        <span>No problems found.</span>
      </div>
    `;
    return html;
  }

  html += `<div style="display:flex;flex-direction:column;gap:2px;">`;
  diags.forEach((d, idx) => {
    const icon =
      d.severity === 'error'
        ? iconSvg('CircleX', 14, g.danger)
        : d.severity === 'warning'
        ? iconSvg('TriangleAlert', 14, g.warning)
        : iconSvg('Info', 14, g.textFaint);
    const lineNum = d.line ?? (d.range?.start?.line != null ? d.range.start.line + 1 : 1);
    html += `
      <button class="dbm-row-btn" data-problem-idx="${idx}" style="height:auto;padding:6px 8px;align-items:flex-start;">
        <span style="padding-top:2px;">${icon}</span>
        <span style="flex:1;display:flex;flex-direction:column;gap:2px;min-width:0;">
          <span style="color:${g.text};word-break:break-word;">${escHtml(d.message)}</span>
          <span style="font-size:11.5px;color:${g.textFaint};">${escHtml(docTitle)} · line ${lineNum}</span>
        </span>
      </button>
    `;
  });
  html += `</div>`;
  return html;
}

/** Renderiza o Inspector lateral direito completo (`shell/panels.rs::inspector`) */
export function renderInspectorPane(params: {
  schema: Schema;
  selection: string[];
  selectedTable: string | null;
  editing: EditTarget | null;
  editingValue: string;
  colTypeW: number;
  enums: string[];
  g: Glass;
}): string {
  const { schema, selection, selectedTable, editing, editingValue, colTypeW, enums, g } = params;

  // Caso 1: Multisseleção (> 1 tabela)
  if (selection.length > 1) {
    let list = '';
    for (const n of selection) {
      list += `
        <div style="display:flex;align-items:center;gap:8px;height:26px;font-size:13.5px;color:${g.text};">
          ${iconSvg('Table2', 14, g.textFaint)}
          <span>${escHtml(n)}</span>
        </div>
      `;
    }
    return `
      <div style="display:flex;flex-direction:column;">
        <div style="display:flex;flex-direction:column;gap:2px;padding:12px;border-bottom:1px solid ${g.edge};">
          <div style="font-size:11.5px;font-weight:600;color:${g.textFaint};">SELECTION</div>
          <div style="font-size:16.5px;font-weight:600;color:${g.text};">${selection.length} tables</div>
          <div style="font-size:13px;color:${g.textMuted};">Shift+click adds or removes</div>
        </div>
        <div class="dbm-section-hdr"><span>TABLES</span></div>
        <div style="display:flex;flex-direction:column;gap:2px;padding:0 12px;">${list}</div>
        <div style="padding:12px;">
          <button data-action="GroupSelection" style="width:100%;height:30px;border-radius:6px;border:none;background:${g.accent};color:${g.mode === 'dark' ? '#0a0a0b' : '#ffffff'};font-weight:600;font-size:13px;display:flex;align-items:center;justify-content:center;gap:6px;cursor:pointer;">
            ${iconSvg('LayoutGrid', 14, 'currentColor')}
            <span>Group selection</span>
          </button>
        </div>
      </div>
    `;
  }

  // Caso 2: 1 Tabela selecionada
  const table = selectedTable ? schema.tables.find((t) => t.name === selectedTable) : null;
  if (table) {
    const group = schema.groups.find((gr) => gr.tables.includes(table.name));
    const refs = schema.refs.filter(
      (r) => r.from.table === table.name || r.to.table === table.name
    );

    const isEditingTable = isSameTarget(editing, { kind: 'table', table: table.name });
    const titleHtml = isEditingTable
      ? `<input class="dbm-insp-input" id="dbm-active-insp-input" value="${escHtml(editingValue)}" />`
      : `<div class="dbm-insp-field" data-start-edit="table" data-table="${escHtml(table.name)}">${escHtml(table.name)}</div>`;

    // Seletor de 8 cores + Sem cor
    const currentHex = (table.color || '').toLowerCase();
    let swatches = `<div style="display:flex;flex-wrap:wrap;gap:6px;padding:4px 12px 0 12px;">`;
    TABLE_COLORS.forEach(([label, hexv]) => {
      const on = currentHex === hexv.toLowerCase();
      swatches += `
        <button title="${escHtml(label)}" data-set-color="${hexv}" data-table="${escHtml(table.name)}" style="width:22px;height:22px;border-radius:999px;background:${hexv};border:2px solid ${on ? g.text : 'transparent'};cursor:pointer;padding:0;"></button>
      `;
    });
    swatches += `
      <button title="No color" data-set-color="" data-table="${escHtml(table.name)}" style="width:22px;height:22px;border-radius:999px;background:transparent;border:1px solid ${g.edge};color:${g.textFaint};display:flex;align-items:center;justify-content:center;cursor:pointer;padding:0;">
        ${iconSvg('X', 12, g.textFaint)}
      </button>
    </div>`;

    // Seção Grupo
    const groupHtml = group
      ? `
        <div style="display:flex;align-items:center;gap:8px;padding:4px 12px 0 12px;">
          ${iconSvg('LayoutGrid', 14, g.textFaint)}
          <span style="flex:1;font-size:13px;color:${g.text};">${escHtml(group.name)}</span>
          <button data-action="UngroupSelection" style="padding:2px 8px;border-radius:5px;border:1px solid ${g.edge};background:transparent;color:${g.textMuted};font-size:12px;cursor:pointer;">Ungroup</button>
        </div>
      `
      : `<div style="padding:4px 12px 0 12px;font-size:12.5px;color:${g.textFaint};">No group · Shift+click other tables and press Ctrl+G to group</div>`;

    // Seção Colunas
    let colsHtml = `
      <div style="display:flex;align-items:center;gap:6px;padding:4px 12px;border-bottom:1px solid ${g.edgeSoft};font-size:10px;font-weight:600;color:${g.textFaint};">
        <div style="flex:1;min-width:60px;">NAME</div>
        <div data-col-splitter="true" title="Drag to resize columns (double-click to reset)" style="width:8px;height:14px;display:flex;align-items:center;justify-content:center;cursor:ew-resize;">
          <div style="width:1.5px;height:10px;background:${g.edge};"></div>
        </div>
        <div style="width:${colTypeW}px;flex:none;">TYPE</div>
        <div style="width:66px;flex:none;">FLAGS</div>
        <div style="width:18px;flex:none;"></div>
      </div>
    `;

    for (const c of table.columns) {
      const editNameTarget: EditTarget = { kind: 'colName', table: table.name, col: c.name };
      const editTypeTarget: EditTarget = { kind: 'colType', table: table.name, col: c.name };
      const isEditingName = isSameTarget(editing, editNameTarget);
      const isEditingType = isSameTarget(editing, editTypeTarget);

      const nameField = isEditingName
        ? `<input class="dbm-insp-input" id="dbm-active-insp-input" value="${escHtml(editingValue)}" />`
        : `<div class="dbm-insp-field" data-start-edit="colName" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}">${escHtml(c.name)}</div>`;

      const typeField = isEditingType
        ? `<input class="dbm-insp-input" id="dbm-active-insp-input" value="${escHtml(editingValue)}" />`
        : `<div class="dbm-insp-field" data-start-edit="colType" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}" style="color:${g.textFaint};font-size:12px;">${escHtml(c.ty)}</div>`;

      colsHtml += `
        <div class="dbm-insp-col-row">
          <div style="flex:1;min-width:60px;overflow:hidden;">${nameField}</div>
          <div data-col-splitter="true" style="width:4px;height:14px;cursor:ew-resize;flex:none;"></div>
          <div style="width:${colTypeW}px;flex:none;overflow:hidden;">${typeField}</div>
          <div style="display:flex;gap:3px;flex:none;">
            <button class="dbm-chip ${c.pk ? 'on' : ''}" title="Primary key" data-toggle-flag="pk" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}" data-val="${!c.pk}">PK</button>
            <button class="dbm-chip ${c.not_null ? 'on' : ''}" title="Required (not null)" data-toggle-flag="not null" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}" data-val="${!c.not_null}">NN</button>
            <button class="dbm-chip ${c.unique ? 'on' : ''}" title="Unique value" data-toggle-flag="unique" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}" data-val="${!c.unique}">UQ</button>
          </div>
          <button class="dbm-insp-col-del" title="Delete column" data-del-col="${escHtml(c.name)}" data-table="${escHtml(table.name)}">
            ${iconSvg('Trash', 13, 'currentColor')}
          </button>
        </div>
      `;

      // Autocomplete de tipos quando estiver editando esta coluna
      if (isEditingType) {
        const q = editingValue.trim().toLowerCase();
        const suggestions: Array<[string, string]> = [];
        for (const [ty, desc] of STANDARD_TYPES) {
          if (!q || ty.toLowerCase().includes(q)) {
            suggestions.push([ty, desc]);
          }
        }
        for (const en of enums) {
          if (!q || en.toLowerCase().includes(q)) {
            suggestions.push([en, 'enumeration']);
          }
        }
        const top = suggestions.slice(0, 8);
        if (top.length > 0) {
          colsHtml += `
            <div id="dbm-type-suggestions" style="margin:4px 12px;padding:6px;border-radius:6px;background:${g.island};border:1px solid ${g.edge};box-shadow:${g.cardShadow};display:flex;flex-direction:column;gap:2px;">
              <div style="display:flex;justify-content:space-between;padding:0 4px 3px 4px;border-bottom:1px solid ${g.edgeSoft};font-size:10px;color:${g.textFaint};">
                <span>Type suggestions (click to apply)</span>
                <span>Esc closes</span>
              </div>
              ${top
                .map(
                  ([sType, sDesc]) => `
                <div data-apply-type="${escHtml(sType)}" data-table="${escHtml(table.name)}" data-col="${escHtml(c.name)}" style="display:flex;justify-content:space-between;align-items:center;padding:4px 6px;border-radius:4px;cursor:pointer;font-size:12px;">
                  <span style="font-weight:500;color:${g.text};">${escHtml(sType)}</span>
                  <span style="font-size:10.5px;color:${g.textFaint};">${escHtml(sDesc)}</span>
                </div>
              `
                )
                .join('')}
            </div>
          `;
        }
      }
    }

    colsHtml += `
      <div style="padding:8px 12px;">
        <button data-add-col-table="${escHtml(table.name)}" style="height:28px;padding:0 10px;border-radius:6px;border:1px solid ${g.edge};background:transparent;color:${g.text};font-size:12.5px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
          ${iconSvg('Plus', 13, g.textMuted)}
          <span>Add column</span>
        </button>
      </div>
    `;

    // Seção Relações
    let relsHtml = `<div style="display:flex;flex-direction:column;gap:4px;padding:4px 12px;">`;
    for (const r of refs) {
      const card =
        r.kind === 'ManyToOne'
          ? 'N:1'
          : r.kind === 'OneToMany'
          ? '1:N'
          : r.kind === 'OneToOne'
          ? '1:1'
          : 'N:N';
      relsHtml += `
        <div style="display:flex;align-items:center;gap:6px;font-size:12.5px;color:${g.textMuted};">
          ${iconSvg('Link2', 13, g.textFaint)}
          <span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escHtml(r.from.table)}.${escHtml(r.from.column)} → ${escHtml(r.to.table)}.${escHtml(r.to.column)}</span>
          <span style="padding:1px 5px;border-radius:4px;border:1px solid ${g.edge};font-size:10.5px;color:${g.textMuted};">${card}</span>
        </div>
      `;
    }
    relsHtml += `</div>`;

    return `
      <div style="display:flex;flex-direction:column;">
        <div style="display:flex;flex-direction:column;gap:2px;padding:12px;border-bottom:1px solid ${g.edge};">
          <div style="font-size:11.5px;font-weight:600;color:${g.textFaint};">TABLE</div>
          <div style="font-size:16.5px;font-weight:600;color:${g.text};">${titleHtml}</div>
          <div style="font-size:12.5px;color:${g.textMuted};">${table.columns.length} columns · F2 to rename</div>
        </div>
        <div class="dbm-section-hdr"><span>COLOR</span></div>
        ${swatches}
        <div class="dbm-section-hdr"><span>GROUP</span></div>
        ${groupHtml}
        <div class="dbm-section-hdr"><span>COLUMNS</span></div>
        ${colsHtml}
        <div class="dbm-section-hdr"><span>RELATIONS</span></div>
        ${relsHtml}
        <div style="padding:12px;margin-top:8px;border-top:1px solid ${g.edgeSoft};">
          <button data-action="DeleteSelection" style="height:28px;padding:0 10px;border-radius:6px;border:1px solid ${hexWithAlpha(g.danger, 0.4)};background:transparent;color:${g.danger};font-size:12.5px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
            ${iconSvg('Trash', 13, g.danger)}
            <span>Delete table</span>
          </button>
        </div>
      </div>
    `;
  }

  // Caso 3: Nada selecionado (`shell/panels.rs`)
  const tips: Array<[string, string[]]> = [
    ['Select', ['V']],
    ['Pan view', ['H']],
    ['Fit to screen', ['Shift', '1']],
    ['Group selection', ['Ctrl', 'G']],
    ['DBML Editor', ['Ctrl', 'E']],
    ['All commands', ['Ctrl', 'K']],
  ];

  return `
    <div style="display:flex;flex-direction:column;gap:6px;padding:16px;">
      <div style="display:flex;justify-content:space-between;align-items:center;">
        <span style="font-size:14.5px;font-weight:600;color:${g.text};">Nothing selected</span>
        <button class="dbm-rail-btn" style="width:26px;height:26px;" title="Hide inspector (Ctrl+I)" data-action="ToggleInspector">
          ${iconSvg('PanelRightClose', 15, g.textMuted)}
        </button>
      </div>
      <div style="font-size:13px;color:${g.textMuted};line-height:1.45;">
        Click a table to view columns, color, group, and relations. Shift+click selects multiple.
      </div>
      <div style="height:1px;margin:8px 0;background:${g.edge};"></div>
      ${tips
        .map(
          ([lbl, kbd]) => `
        <div style="display:flex;justify-content:space-between;align-items:center;height:26px;font-size:13px;color:${g.textMuted};">
          <span>${lbl}</span>
          <span>${renderKbd(kbd)}</span>
        </div>
      `
        )
        .join('')}
    </div>
  `;
}

/** Renderiza o Modal da Paleta de Comandos (`Ctrl+K`) */
export function renderPaletteModal(query: string, g: Glass): string {
  const q = query.trim().toLowerCase();
  const entries = catalog().filter(
    (e) =>
      e.id !== 'OpenPalette' &&
      (!q || e.label.toLowerCase().includes(q) || e.group.toLowerCase().includes(q))
  );

  const groups = new Map<string, typeof entries>();
  for (const e of entries) {
    const list = groups.get(e.group) || [];
    list.push(e);
    groups.set(e.group, list);
  }

  let itemsHtml = '';
  for (const [grp, items] of groups.entries()) {
    itemsHtml += `<div style="padding:8px 12px 4px 12px;font-size:11px;font-weight:600;color:${g.textFaint};">${escHtml(grp.toUpperCase())}</div>`;
    for (const item of items) {
      itemsHtml += `
        <button class="dbm-row-btn" data-action="${item.id as ActionId}" style="width:calc(100% - 12px);justify-content:space-between;height:32px;">
          <span style="color:${g.text};">${escHtml(item.label)}</span>
          <span>${renderKbd(item.displayKeys)}</span>
        </button>
      `;
    }
  }

  return `
    <div class="dbm-modal-backdrop" data-close-modal="true">
      <div class="dbm-modal" onclick="event.stopPropagation()">
        <div style="display:flex;align-items:center;gap:8px;padding:10px 14px;border-bottom:1px solid ${g.edge};">
          ${iconSvg('Search', 15, g.textFaint)}
          <input id="dbm-palette-input" value="${escHtml(query)}" placeholder="Search commands…" style="flex:1;background:transparent;border:none;outline:none;color:${g.text};font-size:14px;" />
          <span class="dbm-kbd">Esc</span>
        </div>
        <div style="flex:1;overflow-y:auto;padding:6px 0;">
          ${itemsHtml || `<div style="padding:16px;text-align:center;color:${g.textMuted};">No commands found.</div>`}
        </div>
      </div>
    </div>
  `;
}

/** Renderiza o Modal de Atalhos de Teclado (`Ctrl+/`) */
export function renderShortcutsModal(g: Glass): string {
  const entries = catalog().filter((e) => e.displayKeys.length > 0);
  return `
    <div class="dbm-modal-backdrop" data-close-modal="true">
      <div class="dbm-modal" onclick="event.stopPropagation()">
        <div style="display:flex;justify-content:space-between;align-items:center;padding:12px 16px;border-bottom:1px solid ${g.edge};">
          <span style="font-size:15px;font-weight:600;color:${g.text};">Keyboard Shortcuts</span>
          <button class="dbm-rail-btn" style="width:26px;height:26px;" data-close-modal="true">${iconSvg('X', 15, g.textMuted)}</button>
        </div>
        <div style="flex:1;overflow-y:auto;padding:12px 16px;display:flex;flex-direction:column;gap:8px;">
          ${entries
            .map(
              (e) => `
            <div style="display:flex;justify-content:space-between;align-items:center;font-size:13px;">
              <span style="color:${g.text};">${escHtml(e.label)}</span>
              <span>${renderKbd(e.displayKeys)}</span>
            </div>
          `
            )
            .join('')}
        </div>
      </div>
    </div>
  `;
}
