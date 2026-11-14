//! Ações do app. Toda ação tem atalho e aparece na paleta de comandos e nos menus.

use gpui_kit::*;

actions!(
    app,
    [
        NewFile,
        OpenFile,
        OpenFolder,
        CloseTab,
        NextTab,
        PrevTab,
        ShowFiles,
        ShowSchema,
        ShowProblems,
        SaveFile,
        Quit,
        OpenPalette,
        ShowShortcuts,
        ToggleTheme,
        ToggleEditor,
        ToggleSidebar,
        ToggleInspector,
        ToggleLegend,
        FitView,
        Relayout,
        ZoomIn,
        ZoomOut,
        ToolSelect,
        ToolHand,
        Deselect,
        ToggleCardTransparency,
        GroupSelection,
        NewTable,
        NewEnum,
        DeleteSelection,
        RenameSelected,
        ResetPanels,
        UngroupSelection,
        FormatDocument,
        MoveLineUp,
        MoveLineDown,
        ZoomInUi,
        ZoomOutUi,
        ResetUiZoom,
        ToggleAutoCollapseInspector,
        ExportSqlPostgres,
        ExportSqlMysql,
        ExportSqlSqlite,
        ExportSqlSqlServer,
        ExportSqlOracle,
    ]
);

/// Contexto de teclado do shell (atalhos com modificador valem na janela toda).
pub const CTX: &str = "Shell";
/// Contexto do canvas focado: atalhos de uma tecla só valem aqui, para não
/// disparar enquanto se digita no editor.
pub const CANVAS: &str = "Canvas";

pub struct Entry {
    pub action: Box<dyn Action>,
    pub group: &'static str,
    pub label: &'static str,
    pub context: &'static str,
    pub binding: Option<KeyBinding>,
}

/// Fonte única para keybindings, paleta de comandos, menus e diálogo de atalhos.
pub fn catalog() -> Vec<Entry> {
    fn e<A: Action + Clone>(
        key: &'static str,
        action: A,
        group: &'static str,
        label: &'static str,
    ) -> Entry {
        // atalhos sem Ctrl só valem com o canvas focado (não disparam ao digitar no editor)
        let context = if key.starts_with("ctrl") || key.starts_with("alt") {
            CTX
        } else {
            CANVAS
        };
        Entry {
            binding: (!key.is_empty()).then(|| KeyBinding::new(key, action.clone(), Some(context))),
            action: Box::new(action),
            group,
            label,
            context,
        }
    }
    vec![
        e("ctrl-n", NewFile, "File", "New File"),
        e("ctrl-o", OpenFile, "File", "Open .dbml File…"),
        e("ctrl-shift-o", OpenFolder, "File", "Open Project Folder…"),
        e("ctrl-w", CloseTab, "File", "Close Tab"),
        e("ctrl-tab", NextTab, "View", "Next Tab"),
        e("ctrl-shift-tab", PrevTab, "View", "Previous Tab"),
        e("ctrl-shift-e", ShowFiles, "View", "Show Project Files"),
        e("ctrl-shift-d", ShowSchema, "View", "Show Schema (Tables)"),
        e("ctrl-shift-m", ShowProblems, "View", "Show Problems"),
        e("ctrl-s", SaveFile, "File", "Save"),
        e("ctrl-q", Quit, "File", "Quit"),
        e("ctrl-t", ToggleTheme, "View", "Toggle Light/Dark Theme"),
        e("ctrl-e", ToggleEditor, "View", "Toggle DBML Editor"),
        e("ctrl-b", ToggleSidebar, "View", "Toggle Sidebar"),
        e("ctrl-i", ToggleInspector, "View", "Toggle Inspector"),
        e("ctrl-shift-l", ToggleLegend, "View", "Toggle Legend"),
        e("v", ToolSelect, "Canvas", "Select Tool"),
        e("h", ToolHand, "Canvas", "Hand Tool (Pan View)"),
        e("shift-1", FitView, "Canvas", "Fit to Screen"),
        e("ctrl-l", Relayout, "Canvas", "Auto-Layout"),
        e("ctrl-=", ZoomIn, "Canvas", "Zoom In"),
        e("ctrl--", ZoomOut, "Canvas", "Zoom Out"),
        e("escape", Deselect, "Canvas", "Clear Selection"),
        e("t", NewTable, "Canvas", "New Table"),
        e("e", NewEnum, "Canvas", "New Enum"),
        e(
            "delete",
            DeleteSelection,
            "Canvas",
            "Delete Selected Tables",
        ),
        e("f2", RenameSelected, "Canvas", "Rename Table"),
        e("ctrl-g", GroupSelection, "Canvas", "Group Selected Tables"),
        e("ctrl-shift-g", UngroupSelection, "Canvas", "Ungroup Table"),
        e(
            "",
            ToggleCardTransparency,
            "Appearance",
            "Translucent Cards",
        ),
        e(
            "ctrl-shift-=",
            ZoomInUi,
            "Appearance",
            "Increase UI Scale (Fonts & Icons)",
        ),
        e(
            "ctrl-shift--",
            ZoomOutUi,
            "Appearance",
            "Decrease UI Scale (Fonts & Icons)",
        ),
        e("ctrl-shift-0", ResetUiZoom, "Appearance", "Reset UI Scale"),
        e(
            "ctrl-shift-i",
            ToggleAutoCollapseInspector,
            "View",
            "Auto-Collapse Inspector When Nothing Selected",
        ),
        e("alt-shift-f", FormatDocument, "Editor", "Format Document"),
        e("alt-up", MoveLineUp, "Editor", "Move Line Up"),
        e("alt-down", MoveLineDown, "Editor", "Move Line Down"),
        e("ctrl-k", OpenPalette, "Help", "Command Palette"),
        e("ctrl-/", ShowShortcuts, "Help", "Keyboard Shortcuts"),
        e("", ExportSqlPostgres, "Export", "Export SQL (PostgreSQL)"),
        e("", ExportSqlMysql, "Export", "Export SQL (MySQL)"),
        e("", ExportSqlSqlite, "Export", "Export SQL (SQLite)"),
        e("", ExportSqlSqlServer, "Export", "Export SQL (SQL Server)"),
        e("", ExportSqlOracle, "Export", "Export SQL (Oracle)"),
    ]
}
