import { beforeEach, describe, expect, it } from 'vitest';
import { associationsFor, syncDefaultEditor } from './editorAssociation.js';
import { ConfigurationTarget } from 'vscode';
import { mockState, resetMock } from './test/vscode-mock.js';

const VIEW = 'dbmodeling.editor';

describe('associationsFor', () => {
  it('canvas: adiciona a associação preservando as demais', () => {
    expect(associationsFor({ '*.png': 'imagePreview.previewEditor' }, 'canvas')).toEqual({
      '*.png': 'imagePreview.previewEditor',
      '*.dbml': VIEW,
    });
    expect(associationsFor(undefined, 'canvas')).toEqual({ '*.dbml': VIEW });
  });

  it('canvas: sem mudança quando já está associado', () => {
    expect(associationsFor({ '*.dbml': VIEW }, 'canvas')).toBeUndefined();
  });

  it('canvas: substitui outra associação de *.dbml (o usuário pediu o canvas)', () => {
    expect(associationsFor({ '*.dbml': 'default' }, 'canvas')).toEqual({ '*.dbml': VIEW });
  });

  it('text: remove só a associação do DB Modeling', () => {
    expect(associationsFor({ '*.dbml': VIEW, '*.png': 'x' }, 'text')).toEqual({ '*.png': 'x' });
    expect(associationsFor({ '*.dbml': VIEW }, 'text')).toEqual({});
  });

  it('text: não mexe quando não há associação nossa', () => {
    expect(associationsFor(undefined, 'text')).toBeUndefined();
    expect(associationsFor({ '*.dbml': 'outro.editor' }, 'text')).toBeUndefined();
  });

  it('formato legado (array) é deixado como está', () => {
    expect(associationsFor([{ viewType: VIEW, filenamePattern: '*.dbml' }], 'canvas')).toBeUndefined();
  });
});

describe('syncDefaultEditor', () => {
  beforeEach(() => resetMock());

  it('padrão "text": não escreve nas configurações', async () => {
    mockState.config['dbmodeling.defaultEditor'] = { defaultValue: 'text' };
    await syncDefaultEditor();
    expect(mockState.configUpdates).toHaveLength(0);
  });

  it('"canvas": grava workbench.editorAssociations no nível do usuário', async () => {
    mockState.config['dbmodeling.defaultEditor'] = { defaultValue: 'text', globalValue: 'canvas' };
    await syncDefaultEditor();
    expect(mockState.configUpdates).toEqual([
      { key: 'workbench.editorAssociations', value: { '*.dbml': VIEW }, target: ConfigurationTarget.Global },
    ]);
  });

  it('voltar para "text" remove a associação', async () => {
    mockState.config['dbmodeling.defaultEditor'] = { defaultValue: 'text', globalValue: 'text' };
    mockState.config['workbench.editorAssociations'] = { globalValue: { '*.dbml': VIEW } };
    await syncDefaultEditor();
    expect(mockState.configUpdates[0].value).toEqual({});
  });

  it('valor desconhecido cai em "text"', async () => {
    mockState.config['dbmodeling.defaultEditor'] = { defaultValue: 'text', globalValue: 'banana' };
    await syncDefaultEditor();
    expect(mockState.configUpdates).toHaveLength(0);
  });
});
