import { describe, expect, it } from 'vitest';
import { catalog } from '../src/actions';
import { diagnosticLabels } from '../src/workbench';
import type { Diagnostic } from '../src/types';

describe('correções v5', () => {
  it('disponibiliza ações e atalhos do desktop', () => {
    expect(catalog().filter (a => ['ShowDiagramView', 'ShowSplitView', 'FilterExplorer'].includes(a.id)).map(a => [a.id, a.key])).toEqual([
      ['ShowDiagramView', 'ctrl-1'], ['ShowSplitView', 'ctrl-2'], ['FilterExplorer', 'ctrl-shift-f'],
    ]);
  });

  it('conta avisos e erros nas mensagens de diagnóstico', () => {
    const diag = (severity: Diagnostic['severity']) => ({ severity }) as Diagnostic;
    expect(diagnosticLabels([])).toEqual({ status: '0 problemas', footer: 'Modelo válido' });
    expect(diagnosticLabels([diag('warning')])).toEqual({ status: '1 problema', footer: '1 aviso' });
    expect(diagnosticLabels([diag('warning'), diag('warning')])).toEqual({ status: '2 problemas', footer: '2 avisos' });
    expect(diagnosticLabels([diag('error'), diag('warning')])).toEqual({ status: '2 problemas', footer: '1 problema' });
  });
});
