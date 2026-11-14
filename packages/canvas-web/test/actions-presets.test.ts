import { describe, it, expect } from 'vitest';
import { COLUMN_PRESET_ACTIONS, catalog } from '../src/actions';

describe('column preset actions', () => {
  it('are in the Edit group of the command palette with stable labels', () => {
    const labels = catalog()
      .filter((e) => e.label.startsWith('Add Column Preset:'))
      .map((e) => [e.group, e.label]);
    expect(labels).toEqual([
      ['Edit', 'Add Column Preset: UUID PK'],
      ['Edit', 'Add Column Preset: BigInt PK'],
      ['Edit', 'Add Column Preset: Timestamps'],
      ['Edit', 'Add Column Preset: Soft Delete'],
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
