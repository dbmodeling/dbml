//! Textos do helper em pt-BR e en (o helper é um processo à parte e não usa o `i18n` do app).

use crate::constants::{LANG_PORTUGUESE, LANG_PRIMARY_MASK, MESSAGE_MAX_CHARS};

/// Idioma das mensagens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Português do Brasil.
    Pt,
    /// Inglês (padrão para os demais idiomas).
    En,
}

impl Lang {
    /// Idioma a partir de um LANGID do Windows (`GetUserDefaultUILanguage`).
    #[must_use]
    pub fn from_langid(langid: u16) -> Self {
        if langid & LANG_PRIMARY_MASK == LANG_PORTUGUESE {
            Self::Pt
        } else {
            Self::En
        }
    }
}

/// Por que a atualização não foi concluída.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// `dbml.exe` em uso (`dbml run` aberto): nada foi trocado.
    CliInUse,
    /// Um job falhou e o app anterior foi restaurado.
    RolledBack,
    /// Um job falhou e a restauração não foi completa.
    Inconsistent,
    /// A atualização foi aplicada, mas o app novo não abriu.
    LaunchFailed,
    /// Não foi possível reconciliar restos de uma atualização anterior.
    RecoveryFailed,
}

/// Texto da janela de progresso.
#[must_use]
pub fn updating(lang: Lang) -> &'static str {
    match lang {
        Lang::Pt => "Atualizando…",
        Lang::En => "Updating…",
    }
}

/// Título da caixa de erro.
#[must_use]
pub fn error_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Pt => "DB Modeling: falha ao atualizar",
        Lang::En => "DB Modeling: update failed",
    }
}

/// Mensagem do problema, truncada em [`MESSAGE_MAX_CHARS`] caracteres.
#[must_use]
pub fn describe(lang: Lang, problem: &Problem, detail: &str) -> String {
    let head = match (lang, problem) {
        (Lang::Pt, Problem::CliInUse) => {
            "O dbml.exe está em uso (dbml run aberto). Feche-o e reinicie o DB Modeling para atualizar. A versão anterior foi mantida."
        }
        (Lang::En, Problem::CliInUse) => {
            "dbml.exe is in use (dbml run is open). Close it and restart DB Modeling to update. The previous version was kept."
        }
        (Lang::Pt, Problem::RolledBack) => {
            "A atualização falhou e a versão anterior foi restaurada. Abra o DB Modeling novamente."
        }
        (Lang::En, Problem::RolledBack) => {
            "The update failed and the previous version was restored. Open DB Modeling again."
        }
        (Lang::Pt, Problem::Inconsistent) => {
            "A atualização falhou e não foi possível restaurar tudo. Reinstale o DB Modeling com o instalador mais recente."
        }
        (Lang::En, Problem::Inconsistent) => {
            "The update failed and not everything could be restored. Reinstall DB Modeling with the latest installer."
        }
        (Lang::Pt, Problem::LaunchFailed) => {
            "A atualização foi concluída, mas o DB Modeling não abriu. Abra-o pelo menu Iniciar."
        }
        (Lang::En, Problem::LaunchFailed) => {
            "The update completed, but DB Modeling did not start. Open it from the Start menu."
        }
        (Lang::Pt, Problem::RecoveryFailed) => {
            "Não foi possível concluir uma atualização anterior. Reinstale o DB Modeling com o instalador mais recente."
        }
        (Lang::En, Problem::RecoveryFailed) => {
            "A previous update could not be completed. Reinstall DB Modeling with the latest installer."
        }
    };
    let text = if detail.is_empty() {
        head.to_owned()
    } else {
        format!("{head}\n\n{detail}")
    };
    text.chars().take(MESSAGE_MAX_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portugues_por_langid_do_windows() {
        assert_eq!(Lang::from_langid(0x0416), Lang::Pt); // pt-BR
        assert_eq!(Lang::from_langid(0x0816), Lang::Pt); // pt-PT
        assert_eq!(Lang::from_langid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_langid(0x040c), Lang::En); // fr-FR
    }

    #[test]
    fn mensagem_do_dbml_cita_o_comando_e_o_rollback() {
        let text = describe(Lang::Pt, &Problem::CliInUse, "");
        assert!(text.contains("dbml.exe") && text.contains("anterior"));
        assert!(describe(Lang::En, &Problem::CliInUse, "").contains("dbml run"));
    }

    #[test]
    fn mensagem_e_truncada_como_no_zed() {
        let long = "x".repeat(2 * MESSAGE_MAX_CHARS);
        let text = describe(Lang::En, &Problem::RolledBack, &long);
        assert_eq!(text.chars().count(), MESSAGE_MAX_CHARS);
    }

    #[test]
    fn todos_os_problemas_tem_texto_nos_dois_idiomas() {
        for problem in [
            Problem::CliInUse,
            Problem::RolledBack,
            Problem::Inconsistent,
            Problem::LaunchFailed,
            Problem::RecoveryFailed,
        ] {
            for lang in [Lang::Pt, Lang::En] {
                assert!(!describe(lang, &problem, "").is_empty());
            }
        }
        assert_ne!(updating(Lang::Pt), updating(Lang::En));
    }
}
