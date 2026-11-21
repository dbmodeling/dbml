/**
 * Catálogo unificado de ações e atalhos de teclado.
 * Espelho de `crates/app/src/actions.rs` (rótulos em pt-BR).
 */

export type ActionId =
  | 'NewFile'
  | 'SaveFile'
  | 'ExportDbml'
  | 'Undo'
  | 'Redo'
  | 'ShowFiles'
  | 'ShowSchema'
  | 'ShowProblems'
  | 'ToggleTheme'
  | 'ThemeLight' | 'ThemeDark' | 'LanguagePortuguese' | 'LanguageEnglish'
  | 'ShowDiagramView'
  | 'ShowSplitView'
  | 'FilterExplorer'
  | 'ToggleEditor'
  | 'ToggleSidebar'
  | 'ToggleInspector'
  | 'ToggleLegend'
  | 'ToggleMinimap'
  | 'FitView'
  | 'Relayout'
  | 'ZoomIn'
  | 'ZoomOut'
  | 'ToolSelect'
  | 'ToolHand'
  | 'Deselect'
  | 'ToggleEdgeStraight'
  | 'ResetEdgeCurve'
  | 'GroupSelection'
  | 'UngroupSelection'
  | 'NewTable'
  | 'NewEnum'
  | 'DeleteSelection'
  | 'RenameSelected'
  | 'FormatDocument'
  | 'MoveLineUp'
  | 'MoveLineDown'
  | 'ZoomInUi'
  | 'ZoomOutUi'
  | 'ResetUiZoom'
  | 'ToggleAutoCollapseInspector'
  | 'OpenPalette'
  | 'QuickSearch'
  | 'ToggleFocusMode'
  | 'CycleFocusDepth'
  | 'ShowShortcuts'
  | 'ExportSvg'
  | 'ExportPng'
  | 'ExportJpeg'
  | 'ExportMermaid'
  | 'ExportDocsHtml'
  | 'ExportDocsMarkdown'
  | 'ExportSqlPostgres'
  | 'ExportSqlMysql'
  | 'ExportSqlSqlite'
  | 'ExportSqlSqlServer'
  | 'ExportSqlOracle'
  | 'ImportSqlDdl'
  | 'ConnectDatabase'
  | 'AddPresetUuidPk'
  | 'AddPresetBigIntPk'
  | 'AddPresetTimestamps'
  | 'AddPresetSoftDelete';

/** Presets de coluna: ação → id do preset e rótulo (iguais a `ColumnPreset` em `dbml-edit`). */
export const COLUMN_PRESET_ACTIONS: { action: ActionId; preset: ColumnPresetId; label: string }[] = [
  { action: 'AddPresetUuidPk', preset: 'uuid_pk', label: 'UUID PK' },
  { action: 'AddPresetBigIntPk', preset: 'bigint_pk', label: 'BigInt PK' },
  { action: 'AddPresetTimestamps', preset: 'timestamps', label: 'Timestamps' },
  { action: 'AddPresetSoftDelete', preset: 'soft_delete', label: 'Soft Delete' },
];

import type { ColumnPresetId } from './types';
import { t } from './i18n';

export type KeyContext = 'Shell' | 'Canvas';

/** Grupos dos dropdowns de exportação (iguais aos de `crates/app/src/actions.rs`). */
export const GROUP_EXPORT_CODE = 'Exportar código';
export const GROUP_EXPORT_IMAGE = 'Exportar imagem';

export interface ActionEntry {
  id: ActionId;
  key: string;
  displayKeys: string[];
  group: string;
  label: string;
  context: KeyContext;
}

export function catalog(): ActionEntry[] {
  const e = (
    key: string,
    displayKeys: string[],
    id: ActionId,
    group: string,
    label: string
  ): ActionEntry => {
    const context: KeyContext =
      key.startsWith('ctrl') || key.startsWith('alt') ? 'Shell' : 'Canvas';
    return { id, key, displayKeys, group: t(group), label: t(label), context };
  };

  return [
    e('ctrl-n', ['Ctrl', 'N'], 'NewFile', 'Arquivo', 'Novo arquivo'),
    e('ctrl-s', ['Ctrl', 'S'], 'SaveFile', 'Arquivo', 'Salvar'),
    e('', [], 'ExportDbml', 'Arquivo', 'Exportar DBML (.dbml)'),
    e('', [], 'ImportSqlDdl', 'Arquivo', 'Importar SQL DDL…'),
    e('', [], 'ConnectDatabase', 'Arquivo', 'Conectar ao banco…'),
    ...COLUMN_PRESET_ACTIONS.map((p) => e('', [], p.action, 'Editar', t('Adicionar predefinição de coluna: {}', p.label))),
    e('ctrl-z', ['Ctrl', 'Z'], 'Undo', 'Editar', 'Desfazer'),
    e('ctrl-shift-z', ['Ctrl', 'Shift', 'Z'], 'Redo', 'Editar', 'Refazer'),
    e('ctrl-shift-e', ['Ctrl', 'Shift', 'E'], 'ShowFiles', 'Exibir', 'Mostrar arquivos do projeto'),
    e('ctrl-shift-d', ['Ctrl', 'Shift', 'D'], 'ShowSchema', 'Exibir', 'Mostrar esquema (tabelas)'),
    e('ctrl-shift-m', ['Ctrl', 'Shift', 'M'], 'ShowProblems', 'Exibir', 'Mostrar problemas'),
    e('', [], 'ThemeLight', 'Aparência', 'Tema: Claro'),
    e('', [], 'ThemeDark', 'Aparência', 'Tema: Escuro'),
    e('', [], 'LanguagePortuguese', 'Aparência', 'Idioma: Português (Brasil)'),
    e('', [], 'LanguageEnglish', 'Aparência', 'Language: English'),
    e('ctrl-t', ['Ctrl', 'T'], 'ToggleTheme', 'Exibir', 'Alternar tema claro/escuro'),
    e('ctrl-1', ['Ctrl', '1'], 'ShowDiagramView', 'Exibir', 'Visão Diagrama'),
    e('ctrl-2', ['Ctrl', '2'], 'ShowSplitView', 'Exibir', 'Visão Dividido · DBML'),
    e('ctrl-shift-f', ['Ctrl', 'Shift', 'F'], 'FilterExplorer', 'Exibir', 'Filtrar tabelas no Explorador'),
    e('ctrl-e', ['Ctrl', 'E'], 'ToggleEditor', 'Exibir', 'Alternar editor DBML'),
    e('ctrl-b', ['Ctrl', 'B'], 'ToggleSidebar', 'Exibir', 'Alternar barra lateral'),
    e('ctrl-i', ['Ctrl', 'I'], 'ToggleInspector', 'Exibir', 'Alternar inspetor'),
    e('ctrl-shift-l', ['Ctrl', 'Shift', 'L'], 'ToggleLegend', 'Exibir', 'Alternar legenda'),
    e('m', ['M'], 'ToggleMinimap', 'Canvas', 'Alternar minimapa'),
    e('v', ['V'], 'ToolSelect', 'Canvas', 'Ferramenta Selecionar'),
    e('h', ['H'], 'ToolHand', 'Canvas', 'Ferramenta Mão (mover a vista)'),
    e('shift-1', ['Shift', '1'], 'FitView', 'Canvas', 'Ajustar à tela'),
    e('ctrl-l', ['Ctrl', 'L'], 'Relayout', 'Canvas', 'Auto-layout'),
    e('ctrl-=', ['Ctrl', '='], 'ZoomIn', 'Canvas', 'Aumentar zoom'),
    e('ctrl--', ['Ctrl', '-'], 'ZoomOut', 'Canvas', 'Diminuir zoom'),
    e('', [], 'ToggleEdgeStraight', 'Relação', 'Relação: reta/curva'),
    e('', [], 'ResetEdgeCurve', 'Relação', 'Relação: restaurar curva automática'),
    e('escape', ['Esc'], 'Deselect', 'Canvas', 'Limpar seleção'),
    e('t', ['T'], 'NewTable', 'Canvas', 'Nova tabela'),
    e('e', ['E'], 'NewEnum', 'Canvas', 'Novo enum'),
    e('delete', ['Del'], 'DeleteSelection', 'Canvas', 'Excluir tabelas selecionadas'),
    e('f2', ['F2'], 'RenameSelected', 'Canvas', 'Renomear tabela'),
    e('ctrl-g', ['Ctrl', 'G'], 'GroupSelection', 'Canvas', 'Agrupar tabelas selecionadas'),
    e('ctrl-shift-g', ['Ctrl', 'Shift', 'G'], 'UngroupSelection', 'Canvas', 'Desagrupar tabela'),
    e(
      'ctrl-shift-=',
      ['Ctrl', 'Shift', '='],
      'ZoomInUi',
      'Aparência',
      'Aumentar escala da interface (fontes e ícones)'
    ),
    e(
      'ctrl-shift--',
      ['Ctrl', 'Shift', '-'],
      'ZoomOutUi',
      'Aparência',
      'Diminuir escala da interface (fontes e ícones)'
    ),
    e('ctrl-shift-0', ['Ctrl', 'Shift', '0'], 'ResetUiZoom', 'Aparência', 'Redefinir escala da interface'),
    e(
      'ctrl-shift-i',
      ['Ctrl', 'Shift', 'I'],
      'ToggleAutoCollapseInspector',
      'Exibir',
      'Recolher inspetor automaticamente sem seleção'
    ),
    e('alt-shift-f', ['Alt', 'Shift', 'F'], 'FormatDocument', 'Editor', 'Formatar documento'),
    e('alt-up', ['Alt', '↑'], 'MoveLineUp', 'Editor', 'Mover linha para cima'),
    e('alt-down', ['Alt', '↓'], 'MoveLineDown', 'Editor', 'Mover linha para baixo'),
    e('', [], 'ExportSqlPostgres', GROUP_EXPORT_CODE, 'Exportar código: PostgreSQL (.sql)'),
    e('', [], 'ExportSqlMysql', GROUP_EXPORT_CODE, 'Exportar código: MySQL (.sql)'),
    e('', [], 'ExportSqlSqlite', GROUP_EXPORT_CODE, 'Exportar código: SQLite (.sql)'),
    e('', [], 'ExportSqlSqlServer', GROUP_EXPORT_CODE, 'Exportar código: SQL Server (.sql)'),
    e('', [], 'ExportSqlOracle', GROUP_EXPORT_CODE, 'Exportar código: Oracle (.sql)'),
    e('', [], 'ExportMermaid', GROUP_EXPORT_CODE, 'Exportar código: Mermaid erDiagram (.mmd)'),
    e('', [], 'ExportDocsHtml', GROUP_EXPORT_CODE, 'Exportar documentação: HTML (.html)'),
    e('', [], 'ExportDocsMarkdown', GROUP_EXPORT_CODE, 'Exportar documentação: Markdown (.md)'),
    e('', [], 'ExportPng', GROUP_EXPORT_IMAGE, 'Exportar imagem: PNG'),
    e('', [], 'ExportJpeg', GROUP_EXPORT_IMAGE, 'Exportar imagem: JPEG'),
    e('', [], 'ExportSvg', GROUP_EXPORT_IMAGE, 'Exportar imagem: SVG vetorial'),
    e('ctrl-k', ['Ctrl', 'K'], 'OpenPalette', 'Ajuda', 'Paleta de comandos'),
    e('ctrl-f', ['Ctrl', 'F'], 'QuickSearch', 'Exibir', 'Busca rápida (tabelas, colunas, enums)'),
    e('f', ['F'], 'ToggleFocusMode', 'Canvas', 'Modo Foco (vizinhança FK)'),
    e('shift-f', ['Shift', 'F'], 'CycleFocusDepth', 'Canvas', 'Modo Foco: alternar 1-hop / 2-hop'),
    e('ctrl-/', ['Ctrl', '/'], 'ShowShortcuts', 'Ajuda', 'Atalhos de teclado'),
  ];
}
