import { beforeEach, describe, expect, it } from 'vitest';
import { DbmlCustomEditorProvider } from './customEditor.js';
import { openEditor } from './test/harness.js';
import { mockState, resetMock, Uri } from './test/vscode-mock.js';

beforeEach(() => resetMock());

describe('DbmlCustomEditorProvider', () => {
  it('ao abrir um .dbml e receber "ready", envia "update" com o texto', async () => {
    mockState.files.set('file:///ws/schema.dbml', new TextEncoder().encode('x'));
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'Table users {\n  id int\n}\n' });
    await h.send({ command: 'ready' });

    const update = h.posted.find((m) => m.type === 'update');
    expect(update).toBeDefined();
    expect(update.source).toBe('Table users {\n  id int\n}\n');
    expect(update.documentId).toBe('file:///ws/schema.dbml');
    expect(update.saved).toBe(true);
    expect(update.files.map((f: { name: string }) => f.name)).toContain('schema.dbml');
  });

  it('carrega o sidecar de layout existente no primeiro update', async () => {
    mockState.files.set(
      'file:///ws/schema.dbml.layout.json',
      new TextEncoder().encode(JSON.stringify({ positions: { users: { x: 1, y: 2 } } }))
    );
    const h = await openEditor(DbmlCustomEditorProvider);
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').layout).toEqual({ positions: { users: { x: 1, y: 2 } } });
  });

  it('apply_patch aplica o patch e confirma com "result"', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 7,
      patch: {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        new_text: 'X',
        expected_source: 'abc',
      },
    });
    expect(h.posted).toContainEqual({ type: 'result', id: 7 });
  });

  it('apply_patch com texto desatualizado responde erro', async () => {
    const h = await openEditor(DbmlCustomEditorProvider, { text: 'abc' });
    await h.send({
      command: 'apply_patch',
      id: 8,
      patch: {
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        new_text: 'X',
        expected_source: 'outro texto',
      },
    });
    const result = h.posted.find((m) => m.type === 'result' && m.id === 8);
    expect(result.error).toMatch(/changed outside/);
  });
});

describe('mock de Uri', () => {
  it('faz roundtrip de parse/toString', () => {
    expect(Uri.parse('file:///a/b.dbml').toString()).toBe('file:///a/b.dbml');
  });
});
