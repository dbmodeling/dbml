//! Todas as strings e números do helper (zero magic strings/numbers fora deste arquivo).

use std::time::Duration;

// --- Layout em disco (relativo ao diretório do app) ------------------------------------------

/// Pasta com a flag de "instalação pronta" (criada pelo Inno em `/update=true`).
pub const UPDATES_DIR: &str = "updates";
/// Flag gravada pelo Inno no fim da instalação; só a existência importa (o conteúdo é a versão).
pub const FLAG_FILE: &str = "versions.txt";
/// Pasta onde o Inno instala em `/update=true` (não toca nos arquivos em uso).
pub const INSTALL_DIR: &str = "install";
/// Pasta temporária dos arquivos antigos durante a troca.
pub const OLD_DIR: &str = "old";
/// Pasta das ferramentas (o helper mora nela).
pub const TOOLS_DIR: &str = "tools";
/// Executável do app.
pub const APP_EXE: &str = "db-modeling.exe";
/// Executável da CLI Web (`dbml run`).
pub const CLI_EXE: &str = "dbml.exe";
/// Ícone da aplicação.
pub const ICON_FILE: &str = "icon.ico";
/// O próprio helper (precisa ser trocado como os demais).
pub const HELPER_EXE: &str = "auto_update_helper.exe";
/// Log do helper (anexado), em `tools\`.
pub const LOG_FILE: &str = "auto_update_helper.log";

/// Arquivo gerenciado: componentes do caminho relativo ao diretório do app.
pub type ManagedFile = &'static [&'static str];

/// Arquivo da CLI (único que o usuário pode ter em uso por outro processo: `dbml run`).
pub const CLI_FILE: ManagedFile = &[CLI_EXE];
/// Arquivos cobertos pela troca, na ordem do plano: app, CLI, ícone e o próprio helper.
pub const MANAGED_FILES: [ManagedFile; 4] =
    [&[APP_EXE], CLI_FILE, &[ICON_FILE], &[TOOLS_DIR, HELPER_EXE]];
/// Arquivo que o Restart Manager precisa liberar (o helper e a CLI **não** entram: o primeiro
/// se fecharia; o segundo vira falha limpa, não encerramento forçado).
pub const RELEASE_FILES: [ManagedFile; 1] = [&[APP_EXE]];
/// Separador usado só para exibir caminhos relativos em mensagens e logs.
pub const DISPLAY_SEPARATOR: &str = "\\";

// --- Tempos ------------------------------------------------------------------------------------

/// Tempo máximo tentando cada job (Zed: 2 s).
pub const JOB_TIMEOUT_MS: u64 = 2_000;
/// Intervalo entre tentativas de um job que falhou por E/S (Zed: 50 ms).
pub const RETRY_INTERVAL_MS: u64 = 50;
/// Quanto esperar o app antigo sair (`--wait-pid`) antes de seguir: a troca funciona mesmo com ele
/// vivo (renomear um `.exe` em execução é permitido), então estourar o prazo não é erro.
pub const WAIT_PID_TIMEOUT_MS: u32 = 30_000;

/// Prazo padrão de um job.
#[must_use]
pub const fn job_timeout() -> Duration {
    Duration::from_millis(JOB_TIMEOUT_MS)
}

/// Intervalo padrão entre tentativas.
#[must_use]
pub const fn retry_interval() -> Duration {
    Duration::from_millis(RETRY_INTERVAL_MS)
}

// --- Argumentos --------------------------------------------------------------------------------

/// `--launch true|false` (ou `--launch=false`): abrir o app depois da troca.
pub const ARG_LAUNCH: &str = "--launch";
/// `--wait-pid <pid>` (ou `--wait-pid=<pid>`): esperar este processo sair antes de trocar.
pub const ARG_WAIT_PID: &str = "--wait-pid";
/// Valor verdadeiro de `--launch`.
pub const LAUNCH_TRUE: &str = "true";
/// Valor falso de `--launch`.
pub const LAUNCH_FALSE: &str = "false";
/// Separador de `--chave=valor`.
pub const ARG_VALUE_SEPARATOR: char = '=';

/// Parâmetros do instalador Inno no modo de atualização (spec, seção 6): silencioso, instala em
/// `{app}\install`, não fecha o app em execução e nunca reinicia a máquina.
pub const INSTALLER_ARGS: [&str; 4] = [
    "/verysilent",
    "/update=true",
    "/nocloseapplications",
    "/norestart",
];

/// Prazo para o instalador silencioso terminar; passado disso o app o encerra e a atualização
/// falha (um instalador travado não pode prender a trava de atualização para sempre).
pub const INSTALLER_TIMEOUT_SECS: u64 = 600;
/// Intervalo entre as conferências de "o instalador terminou?".
pub const INSTALLER_POLL_MS: u64 = 250;

// --- Janela "Atualizando…" (somente Windows) ------------------------------------------------

/// Largura da janela de progresso (px).
pub const DIALOG_WIDTH_PX: i32 = 320;
/// Altura da janela de progresso (px).
pub const DIALOG_HEIGHT_PX: i32 = 96;
/// Pausa entre rodadas da bomba de mensagens da janela (ms).
pub const DIALOG_POLL_MS: u32 = 30;
/// Classe de janela do Windows usada como janela de texto (sem registrar classe própria).
pub const DIALOG_CLASS: &str = "STATIC";
/// `SS_CENTER`: texto centralizado na horizontal.
pub const SS_CENTER: u32 = 0x0000_0001;
/// `SS_CENTERIMAGE`: texto centralizado na vertical.
pub const SS_CENTERIMAGE: u32 = 0x0000_0200;
/// Mensagens de erro são truncadas neste tamanho (Zed: 600 caracteres).
pub const MESSAGE_MAX_CHARS: usize = 600;

// --- Idioma ------------------------------------------------------------------------------------

/// Máscara do idioma primário em um LANGID do Windows.
pub const LANG_PRIMARY_MASK: u16 = 0x03ff;
/// Idioma primário do português (`LANG_PORTUGUESE`).
pub const LANG_PORTUGUESE: u16 = 0x16;

// --- Códigos de saída --------------------------------------------------------------------------

/// Sucesso (inclusive "nada a fazer").
pub const EXIT_OK: u8 = 0;
/// A atualização falhou (houve rollback ou não foi preciso).
pub const EXIT_FAILED: u8 = 1;
/// Argumentos inválidos.
pub const EXIT_USAGE: u8 = 2;
/// Plataforma sem suporte (o helper só roda no Windows).
pub const EXIT_UNSUPPORTED: u8 = 3;

/// Texto do `stderr` quando o binário roda fora do Windows.
pub const UNSUPPORTED_PLATFORM_TEXT: &str = "auto_update_helper só funciona no Windows";
/// Uso mostrado em argumentos inválidos.
pub const USAGE_TEXT: &str =
    "Uso: auto_update_helper [--launch true|false] [--wait-pid <pid>] [argumentos do app...]";
