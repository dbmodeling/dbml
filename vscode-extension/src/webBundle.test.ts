import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';
import { describe, expect, it } from 'vitest';
import { DbmlCustomEditorProvider } from './customEditor.js';
import { webCliHost } from './cliHost.web.js';
import { openEditor } from './test/harness.js';
import { mockState, resetMock, Uri } from './test/vscode-mock.js';

const entry = fileURLToPath(new URL('./extension.web.ts', import.meta.url));

describe('build web', () => {
  it('compila para o navegador sem puxar child_process nem módulos node:', async () => {
    const result = await esbuild.build({
      entryPoints: [entry],
      bundle: true,
      write: false,
      format: 'cjs',
      platform: 'browser',
      external: ['vscode'],
      logLevel: 'silent',
      metafile: true,
    });
    const inputs = Object.keys(result.metafile.inputs);
    expect(inputs.some((i) => i.includes('introspect'))).toBe(false);
    expect(inputs.some((i) => i.includes('cliHost.node'))).toBe(false);
    const code = result.outputFiles[0].text;
    expect(code).not.toMatch(/child_process|require\("node:|require\("fs"\)/);
  });
});

describe('host web', () => {
  it('conexão com banco responde que não está disponível, sem pedir confirmação', async () => {
    resetMock();
    mockState.config['dbmodeling.cliPath'] = { defaultValue: 'dbml', globalValue: '/opt/dbml' };
    const h = await openEditor(DbmlCustomEditorProvider, { cli: webCliHost });
    await h.send({ command: 'check_cli', id: 1 });
    expect(h.posted.find((m) => m.type === 'result' && m.id === 1).error).toMatch(/Web/);
    expect(mockState.messages.warnings).toHaveLength(0);
  });
});

describe('workspace virtual', () => {
  it('lê e grava o sidecar via workspace.fs em esquemas de workspace virtual', async () => {
    resetMock();
    const uri = Uri.parse('vscode-vfs://github/o/r/schema.dbml');
    mockState.workspaceFolders = [{ uri: Uri.parse('vscode-vfs://github/o/r'), name: 'r', index: 0 }];
    mockState.files.set(
      'vscode-vfs://github/o/r/schema.dbml.layout.json',
      new TextEncoder().encode(JSON.stringify({ positions: { a: { x: 1, y: 1 } } }))
    );
    const h = await openEditor(DbmlCustomEditorProvider, { uri });
    await h.send({ command: 'ready' });
    expect(h.posted.find((m) => m.type === 'update').layout).toEqual({ positions: { a: { x: 1, y: 1 } } });
    await h.send({ command: 'save_layout', id: 1, layout: { positions: {} } });
    expect(h.posted).toContainEqual({ type: 'result', id: 1 });
    expect(new TextDecoder().decode(mockState.files.get('vscode-vfs://github/o/r/schema.dbml.layout.json'))).toContain('positions');
  });

  it('documento untitled não tem sidecar', async () => {
    resetMock();
    const h = await openEditor(DbmlCustomEditorProvider, { uri: Uri.parse('untitled:Untitled-1') });
    await h.send({ command: 'save_layout', id: 1, layout: { positions: {} } });
    expect(mockState.files.size).toBe(0);
  });
});
