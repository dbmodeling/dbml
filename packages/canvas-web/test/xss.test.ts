// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeTransport, frame, installDomStubs, mount } from './helpers/harness';
import type { WasmApi } from '../src/types';

const XSS = '<img src=x onerror="window.__xss=1">';

function makeMockWasm(): WasmApi {
  const tableName = `t_${XSS}`;
  const colName = `c_${XSS}`;
  const enumName = `e_${XSS}`;
  const groupName = `g_${XSS}`;
  const diag = {
    severity: 'warning' as const,
    message: `warn_${XSS}`,
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 5 } },
  };
  const schema = {
    tables: [
      {
        name: tableName,
        schema: undefined,
        alias: undefined,
        note: `note_${XSS}`,
        color: undefined,
        columns: [
          {
            name: colName,
            ty: `varchar_${XSS}`,
            pk: true,
            fk: false,
            not_null: true,
            unique: false,
            note: `colnote_${XSS}`,
            default_value: `def_${XSS}`,
            check: `chk_${XSS}`,
          },
        ],
        indexes: [{ name: `idx_${XSS}`, columns: [{ kind: 'column' as const, value: colName }], unique: false, pk: false }],
      },
    ],
    refs: [],
    groups: [{ name: groupName, tables: [tableName] }],
    enums: [{ name: enumName, values: [`val_${XSS}`] }],
    diagnostics: [diag],
  };
  const scene = {
    tables: [{ name: tableName, x: 40, y: 40, w: 230, h: 90, tint: '', fk_columns: [] }],
    edges: [],
    groups: [{ name: groupName, tables: [tableName], color: '#5E6AD2' }],
    enums: [{ name: enumName, x: 320, y: 40, values: [`val_${XSS}`], referenced_by: [tableName] }],
    positions: { [tableName]: { x: 40, y: 40 }, [`enum:${enumName}`]: { x: 320, y: 40 } },
  };
  return {
    parse_and_analyze: () => ({ schema, diagnostics: [diag], enums: [enumName] }),
    compute_layout: () => ({ positions: scene.positions }),
    compute_scene: () => scene,
    apply_command: () => ({ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, byte_start: 0, byte_end: 0, new_text: '' }),
    swap_dbml_lines: () => ({ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, byte_start: 0, byte_end: 0, new_text: '' }),
    format_dbml: (s: string) => s,
  } as unknown as WasmApi;
}

describe('XSS escaping across workbench innerHTML (#39)', () => {
  beforeEach(() => {
    installDomStubs();
    document.body.innerHTML = '';
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as unknown as { __xss?: number }).__xss;
  });

  it('escapes HTML payloads in tables, columns, notes, enums, groups, files and diagnostics', async () => {
    const transport = fakeTransport();
    const { root, workbench } = mount(transport, makeMockWasm());
    transport.update(
      `Table "${XSS}" { id int }`,
      undefined,
      {
        title: `file_${XSS}.dbml`,
        documentId: `doc_${XSS}`,
        files: [{ name: `file_${XSS}.dbml`, path: `/tmp/file_${XSS}.dbml`, active: true }],
        saved: true,
      }
    );
    await frame();

    const anyWb = workbench as unknown as {
      pane: string;
      selection: string[];
      renderAll: () => void;
    };
    anyWb.selection = [`t_${XSS}`];
    for (const pane of ['files', 'schema', 'problems']) {
      anyWb.pane = pane;
      anyWb.renderAll();
    }

    expect(root.querySelectorAll('img')).toHaveLength(0);
    expect((window as unknown as { __xss?: number }).__xss).toBeUndefined();
    expect(root.textContent).toContain(`<img src=x onerror="window.__xss=1">`);
    workbench.destroy();
  });
});
