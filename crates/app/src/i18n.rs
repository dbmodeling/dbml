use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    PtBr,
    En,
}

impl Lang {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pt-BR" => Some(Self::PtBr),
            "en" => Some(Self::En),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PtBr => "pt-BR",
            Self::En => "en",
        }
    }
}

pub fn system() -> Lang {
    // shortcut: Windows without LANG falls back to English; use a system locale API if one is added.
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default();
    if locale.to_ascii_lowercase().starts_with("pt") {
        Lang::PtBr
    } else {
        Lang::En
    }
}

pub fn set(lang: Lang) {
    ENGLISH.store(lang == Lang::En, Ordering::Relaxed);
}
pub fn current() -> Lang {
    if ENGLISH.load(Ordering::Relaxed) {
        Lang::En
    } else {
        Lang::PtBr
    }
}

/// Translate recognized lint templates while leaving quoted identifiers and unknown messages intact.
pub fn diagnostic(message: &str) -> String {
    if current() == Lang::En {
        return message.to_owned();
    }
    diagnostic_pt(message)
}

fn diagnostic_pt(message: &str) -> String {
    const TEMPLATES: &[(&str, &str)] = &[
        ("Duplicate table `{}`", "Tabela duplicada `{}`"),
        (
            "Duplicate column `{}` in `{}`",
            "Coluna duplicada `{}` em `{}`",
        ),
        ("Column `{}` has no type", "A coluna `{}` não possui tipo"),
        ("`{}` has no primary key", "`{}` não possui chave primária"),
        (
            "Unknown type `{}` on column `{}`",
            "Tipo desconhecido `{}` na coluna `{}`",
        ),
        (
            "Unknown type `{}` on column `{}` (did you mean `{}`?)",
            "Tipo desconhecido `{}` na coluna `{}` (você quis dizer `{}`?)",
        ),
        ("Table `{}` does not exist", "A tabela `{}` não existe"),
        (
            "Column `{}` does not exist in `{}`",
            "A coluna `{}` não existe em `{}`",
        ),
        ("Missing `{` after `Table {}`", "Falta `{` após `Table {}`"),
        (
            "Table `{}` was not closed with `}`",
            "A tabela `{}` não foi fechada com `}`",
        ),
        (
            "Closing bracket `]` without matching `[`",
            "Colchete `]` sem um `[` correspondente",
        ),
        (
            "Missing `]` to close settings",
            "Falta `]` para fechar as configurações",
        ),
        (
            "Empty settings block `[]` on column `{}`",
            "Bloco de configurações `[]` vazio na coluna `{}`",
        ),
        (
            "Incomplete setting: expected `primary key` or `pk`",
            "Configuração incompleta: esperado `primary key` ou `pk`",
        ),
        (
            "Incomplete setting: expected `not null`",
            "Configuração incompleta: esperado `not null`",
        ),
        (
            "Setting `{}:` requires a value on column `{}`",
            "A configuração `{}:` exige um valor na coluna `{}`",
        ),
        (
            "Unknown setting `{}` on column `{}` (expected: pk, fk, not null, unique, increment, default, check, note, ref)",
            "Configuração desconhecida `{}` na coluna `{}` (esperado: pk, fk, not null, unique, increment, default, check, note, ref)",
        ),
        (
            "Referenced column `{}` is neither `pk` nor `unique`",
            "A coluna referenciada `{}` não é `pk` nem `unique`",
        ),
        (
            "Unrecognized construct: `{}`",
            "Construção não reconhecida: `{}`",
        ),
    ];
    for (source, target) in TEMPLATES {
        let parts: Vec<_> = source.split("{}").collect();
        let Some(mut rest) = message.strip_prefix(parts[0]) else {
            continue;
        };
        let mut values = Vec::new();
        let mut matched = true;
        for (i, delimiter) in parts.iter().enumerate().skip(1) {
            if i == parts.len() - 1 {
                if let Some(value) = rest.strip_suffix(delimiter) {
                    values.push(value);
                } else {
                    matched = false;
                }
            } else if let Some((value, tail)) = rest.split_once(delimiter) {
                values.push(value);
                rest = tail;
            } else {
                matched = false;
                break;
            }
        }
        if matched {
            let mut pieces = target.split("{}");
            let mut result = pieces.next().unwrap_or_default().to_owned();
            for value in values {
                result.push_str(value);
                result.push_str(pieces.next().unwrap_or_default());
            }
            return result;
        }
    }
    message.to_owned()
}

const TRANSLATIONS: &[(&str, &str)] = &[
    ("Arquivo", "File"),
    (
        "O arquivo mudou no disco. Salve manualmente para confirmar.",
        "The file changed on disk. Save manually to confirm.",
    ),
    ("Excluir FK", "Delete FK"),
    ("Já existem no modelo: {}.", "Already in the model: {}."),
    ("Editar", "Edit"),
    ("Exibir", "View"),
    ("Tabelas", "Tables"),
    ("Aparência", "Appearance"),
    ("Ajuda", "Help"),
    ("Exportar código", "Export code"),
    ("Exportar imagem", "Export image"),
    ("TEMA", "THEME"),
    ("IDIOMA", "LANGUAGE"),
    ("Claro", "Light"),
    ("Escuro", "Dark"),
    ("Translúcido", "Translucent"),
    (
        "A janela deixa ver o que está atrás, desfocado",
        "See the blurred background through the window",
    ),
    ("ESCALA DA UI", "UI SCALE"),
    ("PAINÉIS", "PANELS"),
    (
        "Recolher inspetor automaticamente",
        "Auto-collapse inspector",
    ),
    (
        "Esconde-o quando nada está selecionado",
        "Hide when nothing is selected",
    ),
    ("Propriedades", "Properties"),
    ("Projeto", "Project"),
    ("Esquema", "Schema"),
    ("Problemas", "Problems"),
    ("Nota", "Note"),
    ("Cor", "Color"),
    ("Grupo", "Group"),
    ("LEGENDA", "LEGEND"),
    ("Nada selecionado", "Nothing selected"),
    ("Nenhum problema neste arquivo", "No problems in this file"),
    ("Nenhuma tabela ainda", "No tables yet"),
    (
        "Nenhuma coluna referencia este enum",
        "No columns reference this enum",
    ),
    ("(sem valores)", "(no values)"),
    ("Adicionar coluna", "Add column"),
    ("Adicionar índice", "Add index"),
    ("Adicionar valor", "Add value"),
    ("Agrupar seleção", "Group selection"),
    ("Ajustar à tela", "Fit to screen"),
    ("Alternar inspector", "Toggle inspector"),
    ("Alternar painel lateral", "Toggle sidebar"),
    ("Alternar tema", "Toggle theme"),
    ("Barra lateral", "Sidebar"),
    ("Inspetor", "Inspector"),
    ("Diagrama", "Diagram"),
    ("Dividido · DBML", "Split · DBML"),
    ("Editar expressão check", "Edit check expression"),
    ("Editar nota da tabela", "Edit table note"),
    ("Editar valor padrão", "Edit default value"),
    (
        "Excluir relação e coluna FK",
        "Delete relationship and FK column",
    ),
    ("Fechar aba", "Close tab"),
    ("Fechar", "Close"),
    ("Filtrar tabelas", "Filter tables"),
    ("Minimizar", "Minimize"),
    ("Mostrar problemas", "Show problems"),
    ("Nome do índice", "Index name"),
    ("Novo arquivo", "New file"),
    ("Sem cor", "No color"),
    ("Cancelar", "Cancel"),
    ("Descartar alterações", "Discard changes"),
    ("Salvar", "Save"),
    ("Salvar tudo e sair", "Save all and quit"),
    ("Filtrar atalhos", "Filter shortcuts"),
    ("Filtrar tabelas…", "Filter tables…"),
    ("senha", "password"),
    ("Mais opções", "More options"),
    ("Menu do projeto", "Project menu"),
    ("Paleta de comandos", "Command palette"),
    ("Atalhos de teclado", "Keyboard shortcuts"),
    ("Tour guiado", "Guided tour"),
    ("O que há de novo", "What's new"),
    (
        "Ver novidades desta versão",
        "See what's new in this version",
    ),
    ("Novidades da versão {}", "What's new in v{}"),
    ("Histórico de atualizações", "Release history"),
    ("Passo {} de {}", "Step {} of {}"),
    ("Anterior", "Back"),
    ("Próximo", "Next"),
    ("Concluir", "Finish"),
    ("Pular", "Skip"),
    ("Entendi", "Got it"),
    ("Experimentar", "Try it"),
    ("Novo", "New"),
    ("Melhorado", "Improved"),
    ("Corrigido", "Fixed"),
    (
        "Buscar tabelas, colunas, comandos…",
        "Search tables, columns, commands…",
    ),
    (
        "Buscar tabelas, colunas, comandos (Ctrl+K)",
        "Search tables, columns, commands (Ctrl+K)",
    ),
    (
        "Clique em uma tabela para ver colunas, cor, grupo e relações. Shift+clique seleciona várias.",
        "Click a table to see its columns, color, group and relationships. Shift-click to select multiple tables.",
    ),
    ("Cole um SQL para ver a prévia", "Paste SQL to preview"),
    (
        "Conectando e lendo o esquema…",
        "Connecting and reading schema…",
    ),
    (
        "A entrada excede o limite de 5 MB; divida o dump em arquivos menores.",
        "Input exceeds the 5 MB limit; split the dump into smaller files.",
    ),
    (
        "Sem relações · arraste de uma coluna para outra tabela",
        "No relationships · drag from a column to another table",
    ),
    (
        "Sem índices · a chave primária é indexada automaticamente",
        "No indexes · the primary key is indexed automatically",
    ),
    (
        "Shift+clique adiciona ou remove",
        "Shift-click to add or remove",
    ),
    (
        "Somente leitura: apenas o catálogo é lido, nunca os seus dados.",
        "Read-only: only the catalog is read, never your data.",
    ),
    ("Sugestões", "Suggestions"),
    ("Esc fechar", "Esc close"),
    ("↑↓ navegar", "↑↓ navigate"),
    ("↵ executar", "↵ run"),
    (
        "Confiar no certificado do servidor (autoassinado, ex.: Docker)",
        "Trust the server certificate (self-signed, e.g. Docker)",
    ),
    ("Tema: Claro", "Theme: Light"),
    ("Tema: Escuro", "Theme: Dark"),
    ("Alternar translúcido", "Toggle translucency"),
    ("Abrir arquivo .dbml…", "Open .dbml file…"),
    ("Abrir pasta do projeto…", "Open project folder…"),
    (
        "Adicionar predefinição de coluna: BigInt PK",
        "Add column preset: BigInt PK",
    ),
    (
        "Adicionar predefinição de coluna: Soft Delete",
        "Add column preset: Soft Delete",
    ),
    (
        "Adicionar predefinição de coluna: Timestamps",
        "Add column preset: Timestamps",
    ),
    (
        "Adicionar predefinição de coluna: UUID PK",
        "Add column preset: UUID PK",
    ),
    ("Agrupar tabelas selecionadas", "Group selected tables"),
    ("Alternar filtro do esquema", "Toggle schema filter"),
    ("Alternar tema claro/escuro", "Toggle light/dark theme"),
    (
        "Aumentar escala da interface (fontes e ícones)",
        "Increase interface scale (fonts and icons)",
    ),
    ("Conectar ao banco…", "Connect to database…"),
    ("Desagrupar tabela", "Ungroup table"),
    (
        "Diminuir escala da interface (fontes e ícones)",
        "Decrease interface scale (fonts and icons)",
    ),
    ("Duplicar tabela", "Duplicate table"),
    ("Excluir tabelas selecionadas", "Delete selected tables"),
    (
        "Exportar código: Mermaid erDiagram (.mmd)",
        "Export code: Mermaid erDiagram (.mmd)",
    ),
    ("Exportar código: MySQL (.sql)", "Export code: MySQL (.sql)"),
    (
        "Exportar código: Oracle (.sql)",
        "Export code: Oracle (.sql)",
    ),
    (
        "Exportar código: PostgreSQL (.sql)",
        "Export code: PostgreSQL (.sql)",
    ),
    (
        "Exportar código: SQL Server (.sql)",
        "Export code: SQL Server (.sql)",
    ),
    (
        "Exportar código: SQLite (.sql)",
        "Export code: SQLite (.sql)",
    ),
    ("Exportar imagem: SVG vetorial", "Export image: SVG vector"),
    ("Exportar imagem: PNG", "Export image: PNG"),
    ("Exportar imagem: JPEG", "Export image: JPEG"),
    ("Ferramenta Mão (mover a vista)", "Hand tool (pan view)"),
    ("Ferramenta Selecionar", "Select tool"),
    ("Importar SQL DDL…", "Import SQL DDL…"),
    ("Limpar seleção", "Clear selection"),
    ("Mostrar arquivos do projeto", "Show project files"),
    ("Mostrar esquema (tabelas)", "Show schema (tables)"),
    ("Mover linha para baixo", "Move line down"),
    ("Mover linha para cima", "Move line up"),
    ("Nova tabela", "New table"),
    ("Novo enum", "New enum"),
    ("Próxima aba", "Next tab"),
    ("Aba anterior", "Previous tab"),
    (
        "Recolher inspetor automaticamente sem seleção",
        "Auto-collapse inspector when nothing is selected",
    ),
    (
        "Relação: restaurar curva automática",
        "Relationship: reset automatic curve",
    ),
    ("Relação: reta/curva", "Relationship: straight/curved"),
    ("Renomear tabela", "Rename table"),
    ("Restaurar largura dos painéis", "Reset panel widths"),
    ("Ver tabela no DBML", "Show table in DBML"),
    ("Visão Diagrama", "Diagram view"),
    ("Visão Dividido · DBML", "Split view · DBML"),
    (
        "Visão de código (somente editor)",
        "Code view (editor only)",
    ),
    ("Alternar editor DBML", "Toggle DBML editor"),
    ("Alternar legenda", "Toggle legend"),
    ("Aumentar zoom", "Zoom in"),
    ("Diminuir zoom", "Zoom out"),
    ("Restaurar zoom (100%)", "Reset zoom (100%)"),
    ("Sair", "Quit"),
    ("Desfazer", "Undo"),
    ("Refazer", "Redo"),
    ("Formatar documento", "Format document"),
    ("Redefinir escala da interface", "Reset interface scale"),
    ("Abrir arquivo", "Open file"),
    ("Abrir editor DBML", "Open DBML editor"),
    (
        "Arraste para mover a tabela · Esc limpa a seleção",
        "Drag to move the table · Esc clears the selection",
    ),
    ("Chave primária", "Primary key"),
    (
        "Clique em uma tabela para ver detalhes · arraste o fundo para mover a vista · role para dar zoom",
        "Click a table for details · drag the background to pan · scroll to zoom",
    ),
    ("Conectar ao banco", "Connect to database"),
    (
        "Curvando a relação: solte para gravar",
        "Adjusting relationship curve: release to save",
    ),
    (
        "Escreva seu esquema DBML no editor ou abra um arquivo existente.",
        "Write your DBML schema in the editor or open an existing file.",
    ),
    ("Excluir tabela", "Delete table"),
    ("Ferramentas", "Tools"),
    ("Importar SQL DDL", "Import SQL DDL"),
    (
        "Mão: arraste para mover a vista · V volta a Selecionar",
        "Hand: drag to pan · V returns to Select",
    ),
    ("Mão: mover a vista", "Hand: pan view"),
    ("Não nulo · UQ único", "Not null · UQ unique"),
    (
        "Relação (coluna → coluna)",
        "Relationship (column → column)",
    ),
    (
        "Relação selecionada · arraste a alça ou use as setas para curvar · Delete remove a linha e a coluna FK · Esc limpa a seleção",
        "Relationship selected · drag the handle or use arrow keys to curve · Delete removes the line and FK column · Esc clears the selection",
    ),
    (
        "Selecionando: solte para selecionar as tabelas tocadas",
        "Selecting: release to select intersecting tables",
    ),
    ("Selecionar", "Select"),
    (
        "Tabela selecionada e suas relações",
        "Selected table and its relationships",
    ),
    ("Testar conexão", "Test connection"),
    ("1 erro", "1 error"),
    ("1 problema", "1 problem"),
    ("1 tabela", "1 table"),
    ("AO EXCLUIR", "ON DELETE"),
    ("Abrir arquivo .dbml", "Open .dbml file"),
    ("Abrir pasta", "Open folder"),
    ("Arquivos do projeto", "Project files"),
    ("COLUNAS", "COLUMNS"),
    ("Criar grupo", "Create group"),
    ("EXPORTAR CÓDIGO", "EXPORT CODE"),
    ("EXPORTAR IMAGEM", "EXPORT IMAGE"),
    ("Enum · 1 valor", "Enum · 1 value"),
    ("Escalável, editável", "Scalable, editable"),
    ("Excluir coluna", "Delete column"),
    ("Excluir enum", "Delete enum"),
    ("Excluir relação", "Delete relationship"),
    ("Excluir valor", "Delete value"),
    ("Excluir índice", "Delete index"),
    ("Exportar DBML", "Export DBML"),
    (
        "Exportar código (SQL / Mermaid)…",
        "Export code (SQL / Mermaid)…",
    ),
    (
        "Exportar imagem (PNG / JPEG / SVG)…",
        "Export image (PNG / JPEG / SVG)…",
    ),
    ("Fechar painel", "Close panel"),
    ("GRUPOS", "GROUPS"),
    ("Modelo válido", "Valid model"),
    ("Mover coluna para baixo", "Move column down"),
    ("Mover coluna para cima", "Move column up"),
    (
        "Nenhum arquivo aberto · abra uma pasta de projeto ou inicie um novo esquema",
        "No file open · open a project folder or start a new schema",
    ),
    ("Obrigatório (not null)", "Required (not null)"),
    ("Padrão", "Default"),
    ("RELAÇÕES", "RELATIONSHIPS"),
    ("Raster · fundo sólido", "Raster · solid background"),
    ("Raster · resolução 2×", "Raster · 2× resolution"),
    ("SEM GRUPO", "UNGROUPED"),
    ("Sem grupo", "No group"),
    ("TABELAS", "TABLES"),
    (
        "Tabela · {group_sub} · F2 para renomear",
        "Table · {group_sub} · F2 to rename",
    ),
    ("Tipo", "Type"),
    ("VALORES", "VALUES"),
    ("Valor único", "Unique value"),
    ("ÍNDICES", "INDEXES"),
    ("Abrir", "Open"),
    ("Abrir projeto", "Open project"),
    ("Alterações não salvas", "Unsaved changes"),
    ("Banco", "Database"),
    (
        "Cole instruções CREATE TABLE. A prévia em DBML atualiza enquanto você digita.",
        "Paste CREATE TABLE statements. The DBML preview updates as you type.",
    ),
    ("Colunas", "Columns"),
    ("Conectando…", "Connecting…"),
    ("Conectar", "Connect"),
    ("Confirmar substituição?", "Replace document?"),
    ("Escolher arquivo .sql…", "Choose .sql file…"),
    (
        "Faça a engenharia reversa de um esquema existente para DBML.",
        "Reverse engineer an existing schema into DBML.",
    ),
    (
        "Inspetor: recolher automaticamente sem seleção (ativado)",
        "Inspector: auto-collapse without selection (enabled)",
    ),
    (
        "Inspetor: sempre visível (desativado)",
        "Inspector: always visible (disabled)",
    ),
    (
        "Nome de coluna inválido: use letras, números e _.",
        "Invalid column name: use letters, numbers and _.",
    ),
    (
        "Nome de enum inválido: use letras, números e _.",
        "Invalid enum name: use letters, numbers and _.",
    ),
    (
        "Nome de tabela inválido: use letras, números e _.",
        "Invalid table name: use letters, numbers and _.",
    ),
    ("Prévia DBML", "DBML preview"),
    ("Selecione o arquivo SQL", "Select SQL file"),
    ("Selecione o banco SQLite", "Select SQLite database"),
    (
        "Selecione tabelas (Shift+clique) para agrupar.",
        "Select tables (Shift-click) to group.",
    ),
    (
        "Selecione uma tabela para adicionar a predefinição de coluna",
        "Select a table to add the column preset",
    ),
    ("Usuário", "User"),
    (
        "Valor de enum inválido: use letras, números e _.",
        "Invalid enum value: use letters, numbers and _.",
    ),
    (
        "Você tem alterações não salvas em documentos abertos. Salvar antes de sair?",
        "Open documents have unsaved changes. Save before quitting?",
    ),
    ("nome do banco", "database name"),
    ("porta padrão", "default port"),
    ("usuário", "user"),
    ("expressão…", "expression…"),
    ("Movendo a vista", "Panning view"),
    ("Movendo {}", "Moving {}"),
    ("Movendo {} tabelas", "Moving {} tables"),
    ("Movendo grupo {}", "Moving group {}"),
    ("Movendo enum {}", "Moving enum {}"),
    (
        "Relação de {}.{}: solte na coluna de destino",
        "Relationship from {}.{}: release on the target column",
    ),
    (
        "{} tabelas selecionadas · Ctrl+G agrupa",
        "{} tables selected · Ctrl+G to group",
    ),
    ("{} tabelas", "{} tables"),
    ("{} problemas", "{} problems"),
    ("{} erros", "{} errors"),
    ("{} · linha {}", "{} · line {}"),
    ("Excluir {} tabelas", "Delete {} tables"),
    ("Renomear tabela {}", "Rename table {}"),
    ("Renomear coluna {}", "Rename column {}"),
    (
        "Não foi possível abrir o arquivo: {}",
        "Could not open file: {}",
    ),
    (
        "Não foi possível salvar o arquivo: {}",
        "Could not save file: {}",
    ),
    (
        "Diálogo de arquivo indisponível ({})",
        "File dialog unavailable ({})",
    ),
    (
        "Não foi possível abrir a janela de salvamento. Tente exportar novamente.",
        "Could not open the save dialog. Try exporting again.",
    ),
    (
        "O arquivo está em uso. Feche-o no outro aplicativo ou exporte com outro nome.",
        "The file is in use. Close it in the other app or export with a different name.",
    ),
    (
        "Sem permissão para gravar neste local. Escolha outra pasta.",
        "Permission denied. Choose a different folder.",
    ),
    (
        "O disco está cheio. Libere espaço ou escolha outra pasta.",
        "The disk is full. Free some space or choose a different folder.",
    ),
    (
        "Não foi possível gravar o arquivo. Tente outro nome ou outra pasta.",
        "Could not write the file. Try a different name or folder.",
    ),
    (
        "O arquivo foi movido ou removido. Escolha o arquivo novamente.",
        "The file was moved or removed. Choose the file again.",
    ),
    (
        "Sem acesso ao arquivo. Verifique as permissões ou abra uma cópia.",
        "The file could not be accessed. Check permissions or open a copy.",
    ),
    (
        "Não foi possível abrir o arquivo. Selecione-o novamente.",
        "Could not open the file. Select it again.",
    ),
    (
        "O banco recusou a autenticação. Confira o usuário e a senha.",
        "The database rejected authentication. Check the user and password.",
    ),
    (
        "O banco não respondeu. Confira o host, a porta e se o serviço está rodando.",
        "The database did not respond. Check the host, port, and whether the service is running.",
    ),
    (
        "Não foi possível ler o esquema. Verifique as permissões de leitura do catálogo.",
        "Could not read the schema. Check permissions to read the catalog.",
    ),
    (
        "Não foi possível abrir o banco SQLite. Confira o caminho e as permissões do arquivo.",
        "Could not open the SQLite database. Check the file path and permissions.",
    ),
    (
        "Não foi possível conectar. Confira os dados da conexão e tente novamente.",
        "Could not connect. Check the connection details and try again.",
    ),
    (
        "O arquivo não está em UTF-8. Abra uma cópia salva em UTF-8.",
        "The file is not UTF-8. Open a copy saved as UTF-8.",
    ),
    (
        "O arquivo está em uso. Feche-o no outro aplicativo ou use Salvar como.",
        "The file is in use. Close it in the other app or use Save As.",
    ),
    (
        "O disco está cheio. Libere espaço ou use Salvar como.",
        "The disk is full. Free some space or use Save As.",
    ),
    (
        "Sem permissão para gravar nesta pasta. Use Salvar como para escolher outra.",
        "Permission denied. Use Save As to choose a different folder.",
    ),
    (
        "Suas alterações continuam abertas. Tente salvar novamente ou use Salvar como.",
        "Your changes are still open. Try saving again or use Save As.",
    ),
    (
        "Revise o DBML antes de exportar SQL: {}",
        "Check the DBML before exporting SQL: {}",
    ),
    (
        "O SQLite não aceita o schema {} nesta exportação. Escolha outro dialeto.",
        "SQLite does not support the schema {} in this export. Choose another dialect.",
    ),
    (
        "Escolha outro dialeto para exportar SQL.",
        "Choose another dialect for SQL export.",
    ),
    ("Tente exportar em SVG.", "Try exporting as SVG."),
    ("Falha ao exportar {}: {}", "Failed to export {}: {}"),
    ("Tabela '{}' não encontrada", "Table '{}' not found"),
    ("{}: as colunas já existem", "{}: columns already exist"),
    ("Escala da interface: {}%", "Interface scale: {}%"),
    ("sincronizado", "synchronized"),
    ("aviso", "warning"),
    ("avisos", "warnings"),
    ("Substituir", "Replace"),
    ("Procurar…", "Browse…"),
    ("Porta", "Port"),
    ("Senha", "Password"),
    ("Oracle (em breve)", "Oracle (coming soon)"),
    ("Inserir", "Insert"),
    ("Renomear", "Rename"),
    ("Excluir", "Delete"),
    ("Recolher", "Collapse"),
    ("Expandir", "Expand"),
    ("Atualizar", "Refresh"),
    ("Selecionar todas", "Select all"),
    ("Abrir recente", "Open recent"),
    ("Recentes", "Recent"),
    ("Nenhum arquivo recente", "No recent files"),
    ("Sem seleção", "No selection"),
    ("Nova coluna", "New column"),
    ("Novo índice", "New index"),
    ("Adicionar", "Add"),
    ("Remover", "Remove"),
    ("Tabela", "Table"),
    ("Coluna", "Column"),
    ("Índice", "Index"),
    ("Relação", "Relationship"),
    ("Descrição", "Description"),
    ("Nome", "Name"),
    ("Valores", "Values"),
    ("Índices", "Indexes"),
    ("Relações", "Relationships"),
    ("Grupos", "Groups"),
    ("Arquivo sem título", "Untitled file"),
    ("Sem título", "Untitled"),
    ("Fechar todos", "Close all"),
    ("Copiar", "Copy"),
    ("Cortar", "Cut"),
    ("Colar", "Paste"),
    ("Importar", "Import"),
    ("Exportar", "Export"),
    ("Concluído", "Done"),
    ("Erro", "Error"),
    ("Aviso", "Warning"),
    ("Projeto sem nome", "Untitled project"),
    ("sem grupo", "ungrouped"),
    ("nenhum", "none"),
    ("primário", "primary"),
    ("único", "unique"),
    ("enumeração", "enumeration"),
    ("inteiro de 4 bytes", "4-byte integer"),
    ("inteiro de 8 bytes", "8-byte integer"),
    ("inteiro de 2 bytes", "2-byte integer"),
    ("autoincremento", "auto-increment"),
    ("texto de tamanho variável", "variable-length text"),
    ("texto longo", "long text"),
    ("booleano", "boolean"),
    ("identificador único", "unique identifier"),
    ("data e hora", "date and time"),
    ("data/hora com fuso", "date/time with time zone"),
    ("precisão dupla", "double precision"),
    ("binário", "binary"),
    ("JSON binário", "binary JSON"),
    ("Grupo {}", "Group {}"),
    ("Tabela {}", "Table {}"),
    ("{} comandos", "{} commands"),
    ("Enum · {} valores", "Enum · {} values"),
    ("Abrir {}", "Open {}"),
    ("Editar {}", "Edit {}"),
    (
        "Tabela · {} · F2 para renomear",
        "Table · {} · F2 to rename",
    ),
    ("Exportado {} para {}", "Exported {} to {}"),
    (
        "{} tabela encontrada. Substitua o documento ou insira-a.",
        "{} table found. Replace the document or insert it.",
    ),
    (
        "{} tabelas encontradas. Substitua o documento ou insira-as.",
        "{} tables found. Replace the document or insert them.",
    ),
    (
        "{} tabela · {} relação detectada",
        "{} table · {} relationship detected",
    ),
    (
        "{} tabelas · {} relações detectadas",
        "{} tables · {} relationships detected",
    ),
    ("Legenda", "Legend"),
    ("Ocultar legenda", "Hide legend"),
    (
        "Buscar tabelas, colunas, comandos",
        "Search tables, columns, commands",
    ),
    ("Editar tipo {}", "Edit type {}"),
    ("Cor {}", "Color {}"),
    ("{} em {}", "{} in {}"),
    ("Chave estrangeira (FK)", "Foreign key (FK)"),
    ("Revelar no DBML", "Show in DBML"),
    ("Fechar propriedades", "Close properties"),
    ("Fechar editor", "Close editor"),
    ("Mover a vista", "Pan view"),
    ("Todos os comandos", "All commands"),
    ("CARDINALIDADE", "CARDINALITY"),
    ("AO ATUALIZAR", "ON UPDATE"),
    ("Branch Git {}", "Git branch {}"),
    ("{} tabelas · {} relações", "{} tables · {} relationships"),
    ("problema", "problem"),
    ("problemas", "problems"),
    ("Salvo em {}", "Saved to {}"),
    (
        "Não foi possível ler {}. Uma cópia ficou em {} e o app usa os padrões.",
        "Could not read {}. A copy was kept at {} and the app is using defaults.",
    ),
    (
        "Não foi possível ler {} e a cópia de segurança falhou: {}",
        "Could not read {} and the backup copy failed: {}",
    ),
    ("Não foi possível salvar {}: {}", "Could not save {}: {}"),
    ("arquivo de estado ilegível", "unreadable state file"),
    ("{} (recuperado)", "{} (recovered)"),
    (
        "{} aba(s) com edições não salvas foram recuperadas de uma sessão que terminou de forma inesperada.",
        "{} tab(s) with unsaved edits were recovered from a session that ended unexpectedly.",
    ),
    (
        "Instância única indisponível",
        "Single instance unavailable",
    ),
    ("Abrindo…", "Opening…"),
    (
        "DB Modeling não conseguiu iniciar",
        "DB Modeling could not start",
    ),
    (
        "DB Modeling encerrou com um erro",
        "DB Modeling stopped on an error",
    ),
    (
        "Verifique o driver de vídeo (Vulkan/OpenGL no Linux, DirectX no Windows, Metal no macOS) e abra o app de novo.",
        "Check the graphics driver (Vulkan/OpenGL on Linux, DirectX on Windows, Metal on macOS) and open the app again.",
    ),
    (
        "Não foi possível carregar as fontes embutidas",
        "Could not load the embedded fonts",
    ),
    (
        "Não foi possível abrir a janela (sem backend de GPU utilizável?)",
        "Could not open the window (no usable GPU backend?)",
    ),
    (
        "{} mudou no disco e há edições não salvas aqui. Salvar sobrescreve a versão do disco.",
        "{} changed on disk and has unsaved edits here. Saving overwrites the disk version.",
    ),
    ("{} foi recarregado do disco.", "{} was reloaded from disk."),
    (
        "O arquivo mudou no disco desde que foi aberto. Salve de novo para sobrescrever.",
        "The file changed on disk since it was opened. Save again to overwrite.",
    ),
    (
        "Deseja salvar as alterações feitas em \"{}\"?",
        "Save changes made to \"{}\"?",
    ),
    (
        "todos os schemas (ou: public, sales)",
        "all schemas (e.g. public, sales)",
    ),
    (
        "// A prévia em DBML aparece aqui…",
        "// DBML preview appears here…",
    ),
    (
        "{} tabela · {} relações detectadas",
        "{} table · {} relationships detected",
    ),
    (
        "{} tabelas · {} relação detectada",
        "{} tables · {} relationship detected",
    ),
    (
        "Conectado, mas nenhuma tabela foi encontrada. Verifique o banco e o schema.",
        "Connected, but no tables were found. Check the database and schema.",
    ),
    ("Conectado a {}.", "Connected to {}."),
    (
        "Conectado a {}. Schemas: {} (+{} a mais).",
        "Connected to {}. Schemas: {} (+{} more).",
    ),
    (
        "Conectado a {}. Schemas: {}.",
        "Connected to {}. Schemas: {}.",
    ),
    (
        "sidecar de layout não é um objeto JSON",
        "layout sidecar is not a JSON object",
    ),
    ("agora", "now"),
    ("há {} min", "{} min ago"),
    ("há {} h", "{} h ago"),
    ("ontem", "yesterday"),
    ("fev", "Feb"),
    ("abr", "Apr"),
    ("mai", "May"),
    ("ago", "Aug"),
    ("set", "Sep"),
    ("out", "Oct"),
    ("dez", "Dec"),
    ("jan", "Jan"),
    ("mar", "Mar"),
    ("jun", "Jun"),
    ("jul", "Jul"),
    ("nov", "Nov"),
    ("data", "date"),
    ("hora", "time"),
    ("decimal exato", "exact decimal"),
    ("ponto flutuante", "floating point"),
    (
        "Modelagem visual de esquemas de banco de dados no canvas ou em texto DBML\n",
        "Visual database schema modeling on a canvas or in DBML text\n",
    ),
    (
        "Uso: db-modeling [ARQUIVO.dbml | PASTA]\n",
        "Usage: db-modeling [FILE.dbml | FOLDER]\n",
    ),
    ("Opções:", "Options:"),
    (
        "  -v, -V, --version   Mostra a versão do aplicativo",
        "  -v, -V, --version   Show application version",
    ),
    (
        "  -h, --help          Mostra esta mensagem de ajuda",
        "  -h, --help          Show this help message",
    ),
    ("+{} a mais", "+{} more"),
    ("Chave estrangeira", "Foreign key"),
    ("Cardinalidade: muitos · um", "Cardinality: many · one"),
    ("Duplicar", "Duplicate"),
    ("Ver no DBML", "Show in DBML"),
    ("Desagrupar", "Ungroup"),
    ("Restaurar", "Restore"),
    ("Maximizar", "Maximize"),
    ("REFERENCIADO POR", "REFERENCED BY"),
    ("GERAL", "GENERAL"),
    ("INICIAR", "START"),
    ("RECENTES", "RECENT"),
    ("SVG vetorial", "SVG vector"),
    ("somente leitura", "read-only"),
    ("Modo", "Mode"),
    (
        "não foi possível gravar o layout de {}: {}",
        "could not save layout for {}: {}",
    ),
    ("Criar tabela associativa", "Create junction table"),
    ("TABELA ASSOCIATIVA", "JUNCTION TABLE"),
    ("Criar", "Create"),
];

pub fn t(text: &'static str) -> &'static str {
    translate(current(), text)
}

fn translate(lang: Lang, text: &'static str) -> &'static str {
    if lang == Lang::En {
        TRANSLATIONS
            .iter()
            .find(|(pt, _)| *pt == text)
            .map_or(text, |(_, en)| *en)
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_translation_preserves_technical_details() {
        assert_eq!(
            diagnostic_pt("Unknown type `weird{}` on column `Table`"),
            "Tipo desconhecido `weird{}` na coluna `Table`"
        );
        assert_eq!(
            diagnostic_pt("Duplicate column `a` in `b`"),
            "Coluna duplicada `a` em `b`"
        );
        assert_eq!(
            diagnostic_pt("Unknown type `intt` on column `x` (did you mean `int`?)"),
            "Tipo desconhecido `intt` na coluna `x` (você quis dizer `int`?)"
        );
        assert_eq!(
            diagnostic_pt("unrecognized `Table`"),
            "unrecognized `Table`"
        );
    }

    #[test]
    fn translations_are_unique_nonempty_and_switch() {
        let mut keys = std::collections::HashSet::new();
        for (pt, en) in TRANSLATIONS {
            assert!(keys.insert(pt));
            assert_ne!(*en, "");
        }
        for key in ["Arquivo", "Editar", "Propriedades", "Claro", "Translúcido"] {
            assert_ne!(translate(Lang::En, key), key);
            assert_eq!(translate(Lang::PtBr, key), key);
        }
    }
}
