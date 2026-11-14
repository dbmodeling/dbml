/**
 * Catálogo unificado de ações e atalhos de teclado.
 * Portado 1:1 de `crates/app/src/actions.rs`.
 */

export type ActionId =
  | 'NewFile'
  | 'SaveFile'
  | 'Undo'
  | 'Redo'
  | 'ShowFiles'
  | 'ShowSchema'
  | 'ShowProblems'
  | 'ToggleTheme'
  | 'ToggleEditor'
  | 'ToggleSidebar'
  | 'ToggleInspector'
  | 'ToggleLegend'
  | 'FitView'
  | 'Relayout'
  | 'ZoomIn'
  | 'ZoomOut'
  | 'ToolSelect'
  | 'ToolHand'
  | 'Deselect'
  | 'ToggleCardTransparency'
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
  | 'ShowShortcuts'
  | 'ExportSvg'
  | 'ExportPng'
  | 'ExportJpeg'
  | 'ExportMermaid'
  | 'ExportSqlPostgres'
  | 'ExportSqlMysql'
  | 'ExportSqlSqlite'
  | 'ExportSqlSqlServer'
  | 'ExportSqlOracle'
  | 'ImportSqlDdl';

export type KeyContext = 'Shell' | 'Canvas';

/** Grupos dos dropdowns de exportação (iguais aos de `crates/app/src/actions.rs`). */
export const GROUP_EXPORT_CODE = 'Export Code';
export const GROUP_EXPORT_IMAGE = 'Export Image';

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
    return { id, key, displayKeys, group, label, context };
  };

  return [
    e('ctrl-n', ['Ctrl', 'N'], 'NewFile', 'File', 'New .dbml File'),
    e('ctrl-s', ['Ctrl', 'S'], 'SaveFile', 'File', 'Save'),
    e('', [], 'ImportSqlDdl', 'File', 'Import SQL DDL…'),
    e('ctrl-z', ['Ctrl', 'Z'], 'Undo', 'Edit', 'Undo'),
    e('ctrl-shift-z', ['Ctrl', 'Shift', 'Z'], 'Redo', 'Edit', 'Redo'),
    e('ctrl-shift-e', ['Ctrl', 'Shift', 'E'], 'ShowFiles', 'View', 'Show Project Files'),
    e('ctrl-shift-d', ['Ctrl', 'Shift', 'D'], 'ShowSchema', 'View', 'Show Schema (Tables)'),
    e('ctrl-shift-m', ['Ctrl', 'Shift', 'M'], 'ShowProblems', 'View', 'Show Problems'),
    e('ctrl-t', ['Ctrl', 'T'], 'ToggleTheme', 'View', 'Toggle Light/Dark Theme'),
    e('ctrl-e', ['Ctrl', 'E'], 'ToggleEditor', 'View', 'Toggle DBML Editor'),
    e('ctrl-b', ['Ctrl', 'B'], 'ToggleSidebar', 'View', 'Toggle Sidebar'),
    e('ctrl-i', ['Ctrl', 'I'], 'ToggleInspector', 'View', 'Toggle Inspector'),
    e('ctrl-shift-l', ['Ctrl', 'Shift', 'L'], 'ToggleLegend', 'View', 'Toggle Legend'),
    e('v', ['V'], 'ToolSelect', 'Canvas', 'Select Tool'),
    e('h', ['H'], 'ToolHand', 'Canvas', 'Hand Tool (Pan View)'),
    e('shift-1', ['Shift', '1'], 'FitView', 'Canvas', 'Fit to Screen'),
    e('ctrl-l', ['Ctrl', 'L'], 'Relayout', 'Canvas', 'Auto-Layout'),
    e('ctrl-=', ['Ctrl', '='], 'ZoomIn', 'Canvas', 'Zoom In'),
    e('ctrl--', ['Ctrl', '-'], 'ZoomOut', 'Canvas', 'Zoom Out'),
    e('escape', ['Esc'], 'Deselect', 'Canvas', 'Clear Selection'),
    e('t', ['T'], 'NewTable', 'Canvas', 'New Table'),
    e('', [], 'NewEnum', 'Canvas', 'New Enum'),
    e('delete', ['Del'], 'DeleteSelection', 'Canvas', 'Delete Selected Tables'),
    e('f2', ['F2'], 'RenameSelected', 'Canvas', 'Rename Table'),
    e('ctrl-g', ['Ctrl', 'G'], 'GroupSelection', 'Canvas', 'Group Selected Tables'),
    e('ctrl-shift-g', ['Ctrl', 'Shift', 'G'], 'UngroupSelection', 'Canvas', 'Ungroup Table'),
    e('', [], 'ToggleCardTransparency', 'Appearance', 'Translucent Cards'),
    e(
      'ctrl-shift-=',
      ['Ctrl', 'Shift', '='],
      'ZoomInUi',
      'Appearance',
      'Increase UI Scale (Fonts & Icons)'
    ),
    e(
      'ctrl-shift--',
      ['Ctrl', 'Shift', '-'],
      'ZoomOutUi',
      'Appearance',
      'Decrease UI Scale (Fonts & Icons)'
    ),
    e('ctrl-shift-0', ['Ctrl', 'Shift', '0'], 'ResetUiZoom', 'Appearance', 'Reset UI Scale'),
    e(
      'ctrl-shift-i',
      ['Ctrl', 'Shift', 'I'],
      'ToggleAutoCollapseInspector',
      'View',
      'Auto-Collapse Inspector When Nothing Selected'
    ),
    e('alt-shift-f', ['Alt', 'Shift', 'F'], 'FormatDocument', 'Editor', 'Format Document'),
    e('alt-up', ['Alt', '↑'], 'MoveLineUp', 'Editor', 'Move Line Up'),
    e('alt-down', ['Alt', '↓'], 'MoveLineDown', 'Editor', 'Move Line Down'),
    e('', [], 'ExportSqlPostgres', GROUP_EXPORT_CODE, 'Export Code: PostgreSQL (.sql)'),
    e('', [], 'ExportSqlMysql', GROUP_EXPORT_CODE, 'Export Code: MySQL (.sql)'),
    e('', [], 'ExportSqlSqlite', GROUP_EXPORT_CODE, 'Export Code: SQLite (.sql)'),
    e('', [], 'ExportSqlSqlServer', GROUP_EXPORT_CODE, 'Export Code: SQL Server (.sql)'),
    e('', [], 'ExportSqlOracle', GROUP_EXPORT_CODE, 'Export Code: Oracle (.sql)'),
    e('', [], 'ExportMermaid', GROUP_EXPORT_CODE, 'Export Code: Mermaid erDiagram (.mmd)'),
    e('', [], 'ExportPng', GROUP_EXPORT_IMAGE, 'Export Image: PNG'),
    e('', [], 'ExportJpeg', GROUP_EXPORT_IMAGE, 'Export Image: JPEG'),
    e('', [], 'ExportSvg', GROUP_EXPORT_IMAGE, 'Export Image: Vector SVG'),
    e('ctrl-k', ['Ctrl', 'K'], 'OpenPalette', 'Help', 'Command Palette'),
    e('ctrl-/', ['Ctrl', '/'], 'ShowShortcuts', 'Help', 'Keyboard Shortcuts'),
  ];
}
