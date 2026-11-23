//! Todas as strings e números do updater (zero magic strings/numbers fora deste arquivo).

use std::time::Duration;

// --- Manifesto ---------------------------------------------------------------------------------

/// URL fixa do manifesto estático publicado junto da Release mais recente.
pub const MANIFEST_URL: &str =
    "https://github.com/dbmodeling/dbml/releases/latest/download/latest.json";
/// URL da assinatura minisign do manifesto.
pub const MANIFEST_SIGNATURE_URL: &str =
    "https://github.com/dbmodeling/dbml/releases/latest/download/latest.json.sig";
/// Prefixo (HTTPS) ao qual todo `url` de ativo e o `notes_url` do manifesto precisam pertencer
/// (as Releases do repositório fixado; o aviso pós-reinício do app usa o mesmo prefixo).
pub const RELEASES_URL_PREFIX: &str = "https://github.com/dbmodeling/dbml/releases/";
/// Caracteres proibidos no restante da URL após o prefixo (escape, barra invertida, userinfo).
pub const FORBIDDEN_URL_CHARS: [char; 3] = ['%', '\\', '@'];
/// Sequências proibidas no restante da URL após o prefixo (travessia e caminho vazio).
pub const FORBIDDEN_URL_SEQUENCES: [&str; 2] = ["..", "//"];
/// Único esquema de manifesto que este cliente entende; os demais são ignorados.
pub const SUPPORTED_MANIFEST_SCHEMA: u32 = 1;
/// Tamanho de um SHA-256 em hexadecimal.
pub const SHA256_HEX_LEN: usize = 64;
/// Base numérica de dígitos hexadecimais.
pub const HEX_RADIX: u32 = 16;

// --- Plataformas -------------------------------------------------------------------------------

/// Chave de plataforma: Windows `x86_64`.
pub const PLATFORM_WINDOWS_X86_64: &str = "windows-x86_64";
/// Chave de plataforma: macOS Apple Silicon.
pub const PLATFORM_MACOS_AARCH64: &str = "macos-aarch64";
/// Chave de plataforma: Linux `x86_64`.
pub const PLATFORM_LINUX_X86_64: &str = "linux-x86_64";
/// Valor de `std::env::consts::OS` para Windows.
pub const OS_WINDOWS: &str = "windows";
/// Valor de `std::env::consts::OS` para macOS.
pub const OS_MACOS: &str = "macos";
/// Valor de `std::env::consts::OS` para Linux.
pub const OS_LINUX: &str = "linux";
/// Valor de `std::env::consts::ARCH` para `x86_64`.
pub const ARCH_X86_64: &str = "x86_64";
/// Valor de `std::env::consts::ARCH` para ARM64.
pub const ARCH_AARCH64: &str = "aarch64";

// --- Política ----------------------------------------------------------------------------------

/// Variável de ambiente com a explicação de desativação (override do usuário/empacotador).
pub const ENV_UPDATE_EXPLANATION: &str = "DBM_UPDATE_EXPLANATION";
/// Arquivo instalado pelos `.deb`/`.rpm` com a explicação de desativação.
pub const UPDATE_EXPLANATION_FILE: &str = "/usr/lib/db-modeling/update-explanation";
/// Variáveis que apontam para `%ProgramFiles%` no Windows.
pub const ENV_PROGRAM_FILES: [&str; 3] = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"];
/// Marcador de App Translocation do macOS no caminho do bundle.
pub const APP_TRANSLOCATION_MARKER: &str = "/AppTranslocation/";
/// Prefixo do arquivo temporário usado para testar permissão de escrita.
pub const WRITE_PROBE_PREFIX: &str = ".dbm-write-probe-";
/// Extensão de um bundle macOS.
pub const MACOS_BUNDLE_EXTENSION: &str = "app";
/// Profundidade do `.app` a partir do executável (`<bin>` → `MacOS` → `Contents` → `.app`).
pub const MACOS_BUNDLE_EXE_DEPTH: usize = 3;
/// Prefixo de caminho "verbatim" do Windows, removido antes de comparar caminhos.
pub const VERBATIM_PATH_PREFIX: &str = "\\\\?\\";
/// Separador de caminho do Windows.
pub const WINDOWS_PATH_SEPARATOR: char = '\\';
/// Separador usado na forma normalizada de caminhos para comparação.
pub const NORMALIZED_PATH_SEPARATOR: char = '/';

// --- Máquina de estados: nomes das transições (mensagens de `TransitionError`) -----------------

/// Transição `restore_pending`.
pub const ACTION_RESTORE_PENDING: &str = "restore_pending";
/// Transição `begin_check`.
pub const ACTION_BEGIN_CHECK: &str = "begin_check";
/// Transição `found_update`.
pub const ACTION_FOUND_UPDATE: &str = "found_update";
/// Transição `no_update`.
pub const ACTION_NO_UPDATE: &str = "no_update";
/// Transição `download_verified`.
pub const ACTION_DOWNLOAD_VERIFIED: &str = "download_verified";
/// Transição `installed`.
pub const ACTION_INSTALLED: &str = "installed";
/// Transição `fail`.
pub const ACTION_FAIL: &str = "fail";
/// Transição `dismiss`.
pub const ACTION_DISMISS: &str = "dismiss";

// --- Cache e trava -----------------------------------------------------------------------------

/// Diretório do app dentro do cache do usuário.
pub const CACHE_APP_DIR: &str = "db-modeling";
/// Diretório de atualizações dentro do diretório do app **no cache do usuário**
/// (`<cache>/db-modeling/updates/`): downloads, temporários e a trava. É a única pasta que a
/// limpeza de órfãos toca. NÃO é a pasta `updates\` do Windows em `{app}`, que guarda
/// `versions.txt` e `install\` e pertence ao `auto_update_helper`.
pub const UPDATES_DIR: &str = "updates";
/// Nome do arquivo de trava entre instâncias.
pub const LOCK_FILE_NAME: &str = "lock";
/// Permissão do diretório de updates em Unix (somente o dono).
pub const UPDATES_DIR_UNIX_MODE: u32 = 0o700;
/// Prefixo do arquivo temporário usado para descobrir o uid efetivo do processo.
pub const OWNER_PROBE_PREFIX: &str = ".dbm-owner-probe-";
/// SDDL do diretório de updates no Windows: DACL protegida (`P`, não herda) com controle total
/// para o dono do objeto (`OW`) e para `SY` (`SYSTEM`), herdado por subpastas e arquivos (`OICI`).
#[cfg(windows)]
pub const DIR_ACL_SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)";

// --- Agendamento -------------------------------------------------------------------------------

/// Segundos em uma hora.
pub const SECONDS_PER_HOUR: u64 = 3600;
/// Intervalo entre checagens automáticas (1 h).
pub const CHECK_INTERVAL: Duration = Duration::from_secs(SECONDS_PER_HOUR);
/// Idade a partir da qual downloads e temporários órfãos são apagados (24 h).
pub const ORPHAN_MAX_AGE: Duration = Duration::from_secs(24 * SECONDS_PER_HOUR);
/// Tempo em que o estado "Atualizado" fica visível após checagem manual (4 s).
pub const UP_TO_DATE_VISIBLE_FOR: Duration = Duration::from_secs(4);

// --- Verificação -------------------------------------------------------------------------------

/// Tamanho do buffer de leitura ao calcular o hash de arquivos (64 KiB).
pub const HASH_READ_BUFFER_BYTES: usize = 64 * 1024;
/// Dígitos hexadecimais por byte.
pub const HEX_DIGITS_PER_BYTE: usize = 2;

// --- Assinatura --------------------------------------------------------------------------------

/// Chave pública minisign primária (somente a linha base64).
pub const PUBLIC_KEY_PRIMARY: &str = "RWQgpCBvQEYx323M6DTxEKY0LM8DpWVnbRvAm7baWHjINu2JLeB37Nes";
/// Chave pública minisign reserva, privada guardada offline (somente a linha base64).
pub const PUBLIC_KEY_RESERVE: &str = "RWSz3CXguSfrgXWWcp6qf/hzd7e6A1ZkE7SloeFJfFYP7Pd/UT2wRja1";
/// As duas chaves aceitas; basta uma verificar a assinatura.
pub const PUBLIC_KEYS: [&str; 2] = [PUBLIC_KEY_PRIMARY, PUBLIC_KEY_RESERVE];
