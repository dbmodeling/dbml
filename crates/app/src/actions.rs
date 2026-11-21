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
        Undo,
        Redo,
        Quit,
        OpenPalette,
        ShowShortcuts,
        ToggleTheme,
        ThemeLight,
        ThemeDark,
        ToggleEditor,
        ShowDiagramView,
        ShowSplitView,
        ShowCodeView,
        FilterExplorer,
        ToggleSidebar,
        ToggleInspector,
        ToggleLegend,
        FitView,
        Relayout,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        ToolSelect,
        ToolHand,
        Deselect,
        LangPortuguese,
        LangEnglish,
        GroupSelection,
        NewTable,
        NewEnum,
        DuplicateTable,
        RevealInDbml,
        EditTableNote,
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
        ExportMermaid,
        ExportPng,
        ExportJpeg,
        ExportSvg,
        ImportSqlDdl,
        ConnectDatabase,
        AddPresetUuidPk,
        AddPresetBigIntPk,
        AddPresetTimestamps,
        AddPresetSoftDelete,
        ToggleEdgeStraight,
        ResetEdgeCurve,
    ]
);

/// Grupo do catálogo / dropdown da barra: exportação de código (SQL por banco, Mermaid).
pub const GROUP_EXPORT_CODE: &str = "Exportar código";
/// Grupo do catálogo / dropdown da barra: exportação de imagem (PNG, JPEG, SVG).
pub const GROUP_EXPORT_IMAGE: &str = "Exportar imagem";

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
            label: crate::i18n::t(label),
            context,
        }
    }
    vec![
        e("ctrl-n", NewFile, "Arquivo", "Novo arquivo"),
        e("ctrl-o", OpenFile, "Arquivo", "Abrir arquivo .dbml…"),
        e(
            "ctrl-shift-o",
            OpenFolder,
            "Arquivo",
            "Abrir pasta do projeto…",
        ),
        e("ctrl-w", CloseTab, "Arquivo", "Fechar aba"),
        e("ctrl-tab", NextTab, "Exibir", "Próxima aba"),
        e("ctrl-shift-tab", PrevTab, "Exibir", "Aba anterior"),
        e(
            "ctrl-shift-e",
            ShowFiles,
            "Exibir",
            "Mostrar arquivos do projeto",
        ),
        e(
            "ctrl-shift-d",
            ShowSchema,
            "Exibir",
            "Mostrar esquema (tabelas)",
        ),
        e("ctrl-shift-m", ShowProblems, "Exibir", "Mostrar problemas"),
        e("ctrl-s", SaveFile, "Arquivo", "Salvar"),
        e("ctrl-z", Undo, "Editar", "Desfazer"),
        e("ctrl-shift-z", Redo, "Editar", "Refazer"),
        e("ctrl-q", Quit, "Arquivo", "Sair"),
        e(
            "ctrl-t",
            ToggleTheme,
            "Exibir",
            "Alternar tema claro/escuro",
        ),
        e("ctrl-e", ToggleEditor, "Exibir", "Alternar editor DBML"),
        e("ctrl-1", ShowDiagramView, "Exibir", "Visão Diagrama"),
        e("ctrl-2", ShowSplitView, "Exibir", "Visão Dividido · DBML"),
        e(
            "ctrl-alt-e",
            ShowCodeView,
            "Exibir",
            "Visão de código (somente editor)",
        ),
        e(
            "ctrl-shift-f",
            FilterExplorer,
            "Exibir",
            "Alternar filtro do esquema",
        ),
        e("ctrl-b", ToggleSidebar, "Exibir", "Alternar painel lateral"),
        e("ctrl-i", ToggleInspector, "Exibir", "Alternar inspector"),
        e("ctrl-shift-l", ToggleLegend, "Exibir", "Alternar legenda"),
        e(
            "ctrl-alt-0",
            ResetPanels,
            "Exibir",
            "Restaurar largura dos painéis",
        ),
        e("v", ToolSelect, "Canvas", "Ferramenta Selecionar"),
        e("h", ToolHand, "Canvas", "Ferramenta Mão (mover a vista)"),
        e("shift-1", FitView, "Canvas", "Ajustar à tela"),
        e("ctrl-l", Relayout, "Canvas", "Auto-layout"),
        e("ctrl-=", ZoomIn, "Canvas", "Aumentar zoom"),
        e("ctrl--", ZoomOut, "Canvas", "Diminuir zoom"),
        e("shift-0", ResetZoom, "Canvas", "Restaurar zoom (100%)"),
        e("escape", Deselect, "Canvas", "Limpar seleção"),
        e("", ToggleEdgeStraight, "Canvas", "Relação: reta/curva"),
        e(
            "",
            ResetEdgeCurve,
            "Canvas",
            "Relação: restaurar curva automática",
        ),
        e("t", NewTable, "Tabelas", "Nova tabela"),
        e("e", NewEnum, "Tabelas", "Novo enum"),
        e(
            "delete",
            DeleteSelection,
            "Tabelas",
            "Excluir tabelas selecionadas",
        ),
        e("f2", RenameSelected, "Tabelas", "Renomear tabela"),
        e("ctrl-d", DuplicateTable, "Tabelas", "Duplicar tabela"),
        e("ctrl-alt-d", RevealInDbml, "Exibir", "Ver tabela no DBML"),
        e(
            "ctrl-alt-n",
            EditTableNote,
            "Tabelas",
            "Editar nota da tabela",
        ),
        e(
            "ctrl-g",
            GroupSelection,
            "Tabelas",
            "Agrupar tabelas selecionadas",
        ),
        e(
            "ctrl-shift-g",
            UngroupSelection,
            "Tabelas",
            "Desagrupar tabela",
        ),
        e("", ThemeLight, "Aparência", "Tema: Claro"),
        e("", ThemeDark, "Aparência", "Tema: Escuro"),
        e("", LangPortuguese, "Aparência", "Idioma: Português (Brasil)"),
        e("", LangEnglish, "Aparência", "Language: English"),
        e(
            "ctrl-shift-=",
            ZoomInUi,
            "Aparência",
            "Aumentar escala da interface (fontes e ícones)",
        ),
        e(
            "ctrl-shift--",
            ZoomOutUi,
            "Aparência",
            "Diminuir escala da interface (fontes e ícones)",
        ),
        e(
            "ctrl-shift-0",
            ResetUiZoom,
            "Aparência",
            "Redefinir escala da interface",
        ),
        e(
            "ctrl-shift-i",
            ToggleAutoCollapseInspector,
            "Exibir",
            "Recolher inspetor automaticamente sem seleção",
        ),
        e(
            "alt-shift-f",
            FormatDocument,
            "Editor",
            "Formatar documento",
        ),
        e("alt-up", MoveLineUp, "Editor", "Mover linha para cima"),
        e("alt-down", MoveLineDown, "Editor", "Mover linha para baixo"),
        e("ctrl-k", OpenPalette, "Ajuda", "Paleta de comandos"),
        e("ctrl-/", ShowShortcuts, "Ajuda", "Atalhos de teclado"),
        e(
            "",
            ExportSqlPostgres,
            GROUP_EXPORT_CODE,
            "Exportar código: PostgreSQL (.sql)",
        ),
        e(
            "",
            ExportSqlMysql,
            GROUP_EXPORT_CODE,
            "Exportar código: MySQL (.sql)",
        ),
        e(
            "",
            ExportSqlSqlite,
            GROUP_EXPORT_CODE,
            "Exportar código: SQLite (.sql)",
        ),
        e(
            "",
            ExportSqlSqlServer,
            GROUP_EXPORT_CODE,
            "Exportar código: SQL Server (.sql)",
        ),
        e(
            "",
            ExportSqlOracle,
            GROUP_EXPORT_CODE,
            "Exportar código: Oracle (.sql)",
        ),
        e(
            "",
            ExportMermaid,
            GROUP_EXPORT_CODE,
            "Exportar código: Mermaid erDiagram (.mmd)",
        ),
        e("", ExportPng, GROUP_EXPORT_IMAGE, "Exportar imagem: PNG"),
        e("", ExportJpeg, GROUP_EXPORT_IMAGE, "Exportar imagem: JPEG"),
        e(
            "",
            ExportSvg,
            GROUP_EXPORT_IMAGE,
            "Exportar imagem: SVG vetorial",
        ),
        e("ctrl-alt-i", ImportSqlDdl, "Arquivo", "Importar SQL DDL…"),
        e(
            "ctrl-alt-c",
            ConnectDatabase,
            "Arquivo",
            "Conectar ao banco…",
        ),
        e(
            "",
            AddPresetUuidPk,
            "Editar",
            "Adicionar predefinição de coluna: UUID PK",
        ),
        e(
            "",
            AddPresetBigIntPk,
            "Editar",
            "Adicionar predefinição de coluna: BigInt PK",
        ),
        e(
            "",
            AddPresetTimestamps,
            "Editar",
            "Adicionar predefinição de coluna: Timestamps",
        ),
        e(
            "",
            AddPresetSoftDelete,
            "Editar",
            "Adicionar predefinição de coluna: Soft Delete",
        ),
    ]
}
