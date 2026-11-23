#![deny(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assert_is_empty
    )
)]
//! Helper do auto-update do App Desktop no Windows (modelo `auto_update_helper` do Zed).
//!
//! O Inno (`/update=true`) instala a versão nova em `{app}\install` e grava
//! `{app}\updates\versions.txt`. Quando o app sai (hook de encerramento: `--launch false`) ou o
//! usuário escolhe "Reiniciar para atualizar" (`--launch true`), o helper:
//!
//! 1. espera o app sair (`--wait-pid`) e libera os handles com o Restart Manager;
//! 2. move os arquivos antigos para `old\`, move os de `install\` para o lugar e apaga `updates`,
//!    `install` e `old` (jobs com `apply`/`rollback`, prazo de 2 s, repetição a cada 50 ms);
//! 3. em qualquer falha, desfaz em ordem inversa e **não** relança o app.
//!
//! A lógica (argumentos, plano de jobs, rollback, recuperação de restos, fluxo) é pura e roda em
//! qualquer SO, testada com pastas temporárias e falhas injetadas. Só [`win`] (Restart Manager,
//! janela "Atualizando…", `MessageBox`) e o binário são do Windows. O app (`crates/app`) usa as
//! partes puras ([`recovery`], [`layout`], [`args`], [`constants`]) para detectar a instalação
//! pendente na abertura e chamar o helper.

pub mod args;
pub mod constants;
pub mod driver;
pub mod fsops;
pub mod jobs;
pub mod layout;
pub mod logfile;
pub mod messages;
pub mod recovery;
pub mod wait;

#[cfg(test)]
mod testkit;
#[cfg(windows)]
mod win;

/// Ponto de entrada do binário `auto_update_helper`.
#[cfg(windows)]
#[must_use]
pub fn main_entry() -> std::process::ExitCode {
    win::main_entry()
}

/// Fora do Windows o helper não faz nada (o binário só existe para o `cargo build` do workspace).
#[cfg(not(windows))]
#[must_use]
pub fn main_entry() -> std::process::ExitCode {
    eprintln!("{}", constants::UNSUPPORTED_PLATFORM_TEXT);
    std::process::ExitCode::from(constants::EXIT_UNSUPPORTED)
}
