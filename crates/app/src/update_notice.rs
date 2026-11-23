//! Aviso "Atualizado para X" do primeiro start depois do reinício.
//!
//! O processo que instalou a versão grava `versão\nurl-das-notas\n` na pasta de configuração; o
//! processo novo lê e apaga o arquivo e mostra o aviso uma vez, junto das novidades que o fluxo
//! existente (`last_seen_version`, `guide::decide_startup_guide`) já abre. Um arquivo separado
//! evita brigar com `settings.txt`, que o `Shell` regrava inteiro a partir da própria cópia.

use std::path::{Path, PathBuf};

use updater::constants::RELEASES_URL_PREFIX;
use updater::manifest::validate_url;

use crate::constants::{UPDATE_NOTICE_FILE, UPDATE_NOTICE_MAX_BYTES};

/// Versão instalada e link das notas (validado pelo updater: pertence ao repositório fixado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNotice {
    /// Versão que o processo antigo instalou.
    pub version: String,
    /// Link das notas da versão.
    pub notes_url: String,
}

impl UpdateNotice {
    /// Texto do arquivo: duas linhas.
    #[must_use]
    pub fn serialize(&self) -> String {
        format!("{}\n{}\n", self.version, self.notes_url)
    }

    /// Lê o texto do arquivo; qualquer formato inesperado é descartado (`None`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines().map(str::trim);
        let version = lines.next().filter(|line| !line.is_empty())?;
        let notes_url = lines.next().filter(|line| !line.is_empty())?;
        // só https dentro das Releases do projeto (mesma validação do manifesto: sem @, %, \, ..
        // nem //): o arquivo é editável pelo usuário e o link é aberto no navegador
        if validate_url(notes_url, RELEASES_URL_PREFIX).is_err()
            || lines.next().is_some_and(|extra| !extra.is_empty())
        {
            return None;
        }
        Some(Self {
            version: version.to_owned(),
            notes_url: notes_url.to_owned(),
        })
    }

    /// O aviso vale para a versão em execução? (Atualização feita à mão depois, ou arquivo
    /// antigo de uma versão que não chegou a ser aberta: não anuncia.)
    #[must_use]
    pub fn is_for(&self, running_version: &str) -> bool {
        self.version == running_version
    }
}

fn path_in(config_dir: &Path) -> PathBuf {
    config_dir.join(UPDATE_NOTICE_FILE)
}

/// Grava o aviso (processo que instalou). Erro de E/S é devolvido: a atualização não depende dele.
///
/// # Errors
/// Falha ao gravar o arquivo.
pub fn save(config_dir: &Path, notice: &UpdateNotice) -> std::io::Result<()> {
    crate::session::atomic_write_sync(&path_in(config_dir), &notice.serialize())
}

/// Lê e **apaga** o aviso (processo novo). Devolve só se valer para `running_version`.
#[must_use]
pub fn take(config_dir: &Path, running_version: &str) -> Option<UpdateNotice> {
    let path = path_in(config_dir);
    let too_big = std::fs::metadata(&path).is_ok_and(|meta| meta.len() > UPDATE_NOTICE_MAX_BYTES);
    let text = if too_big {
        None
    } else {
        std::fs::read_to_string(&path).ok()
    };
    // apaga sempre: um aviso ilegível ou de outra versão não pode reaparecer
    let _ = std::fs::remove_file(&path);
    UpdateNotice::parse(&text?).filter(|notice| notice.is_for(running_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagina_das_releases_fica_sob_o_prefixo_do_updater() {
        assert!(validate_url(crate::constants::RELEASES_URL, RELEASES_URL_PREFIX).is_ok());
    }

    #[test]
    fn notes_url_aceita_pelo_manifesto_e_aceita_pelo_aviso() {
        // as duas pontas usam `updater::manifest::validate_url` com `RELEASES_URL_PREFIX`
        let accepted = "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0";
        assert!(UpdateNotice::parse(&format!("0.4.0\n{accepted}\n")).is_some());
        for rejected in [
            "https://github.com/dbmodeling/dbml/issues/1",
            "https://github.com/dbmodeling/dbml/releases/@evil.example/x",
            "https://github.com/dbmodeling/dbml/releases/../../outro/repo",
            "https://github.com/dbmodeling/dbml/releases/%2e%2e/x",
            "https://github.com/dbmodeling/dbml/releases//x",
        ] {
            assert_eq!(
                UpdateNotice::parse(&format!("0.4.0\n{rejected}\n")),
                None,
                "deveria recusar {rejected}"
            );
        }
    }

    fn notice() -> UpdateNotice {
        UpdateNotice {
            version: "0.4.0".to_owned(),
            notes_url: "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0".to_owned(),
        }
    }

    #[test]
    fn serializa_e_le_de_volta() {
        assert_eq!(UpdateNotice::parse(&notice().serialize()), Some(notice()));
    }

    #[test]
    fn formato_invalido_e_descartado() {
        assert_eq!(UpdateNotice::parse(""), None);
        assert_eq!(UpdateNotice::parse("0.4.0\n"), None);
        assert_eq!(UpdateNotice::parse("\nhttps://x\n"), None);
        // link fora do repositório (arquivo editado) não é aberto
        assert_eq!(UpdateNotice::parse("0.4.0\nhttps://evil.example/x\n"), None);
        assert_eq!(UpdateNotice::parse("0.4.0\nfile:///etc/passwd\n"), None);
        assert_eq!(
            UpdateNotice::parse(
                "0.4.0\nhttps://github.com/dbmodeling/dbml/releases/tag/v0.4.0\nlixo\n"
            ),
            None
        );
    }

    #[test]
    fn so_vale_para_a_versao_em_execucao() {
        assert!(notice().is_for("0.4.0"));
        assert!(!notice().is_for("0.3.4"));
    }

    #[test]
    fn take_le_uma_vez_e_apaga() {
        let dir = tempfile::tempdir().expect("tempdir");
        save(dir.path(), &notice()).expect("save");
        assert_eq!(take(dir.path(), "0.4.0"), Some(notice()));
        assert_eq!(take(dir.path(), "0.4.0"), None);
        assert!(!dir.path().join(UPDATE_NOTICE_FILE).exists());
    }

    #[test]
    fn take_de_outra_versao_apaga_sem_anunciar() {
        let dir = tempfile::tempdir().expect("tempdir");
        save(dir.path(), &notice()).expect("save");
        assert_eq!(take(dir.path(), "0.3.4"), None);
        assert!(!dir.path().join(UPDATE_NOTICE_FILE).exists());
    }

    #[test]
    fn take_sem_arquivo_nao_falha() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(take(dir.path(), "0.4.0"), None);
    }
}
