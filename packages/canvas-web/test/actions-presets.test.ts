import { describe, it, expect } from 'vitest';
import { COLUMN_PRESET_ACTIONS, catalog } from '../src/actions';

describe('column preset actions', () => {
  it('are in the Edit group of the command palette with stable labels', () => {
    const labels = catalog()
      .filter((e) => e.label.startsWith('Adicionar predefinição de coluna:'))
      .map((e) => [e.group, e.label]);
    expect(labels).toEqual([
      ['Editar', 'Adicionar predefinição de coluna: UUID PK'],
      ['Editar', 'Adicionar predefinição de coluna: BigInt PK'],
      ['Editar', 'Adicionar predefinição de coluna: Timestamps'],
      ['Editar', 'Adicionar predefinição de coluna: Soft Delete'],
    ]);
  });

  it('map to the preset ids understood by dbml-edit', () => {
    expect(COLUMN_PRESET_ACTIONS.map((p) => p.preset)).toEqual([
      'uuid_pk',
      'bigint_pk',
      'timestamps',
      'soft_delete',
    ]);
  });
});
