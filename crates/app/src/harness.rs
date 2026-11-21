//! Variáveis de ambiente do harness de screenshot/testes (`DBM_SIZE`, `DBM_MODE`, `DBM_SELECT`,
//! `DBM_PANE`, `DBM_SIDEBAR`, `DBM_EDITOR`, `DBM_INSPECTOR`).
//!
//! Só valem em build de debug ou com a feature `harness`; o binário de release instalado as
//! ignora, para que uma variável perdida no ambiente do usuário não mude o comportamento do app.
//! `DBM_LANG` e `DBM_NO_ANIM` são preferências de uso e continuam valendo sempre.

/// `true` quando o harness está habilitado neste build.
pub const ENABLED: bool = cfg!(any(debug_assertions, feature = "harness"));

fn gate(enabled: bool, value: Option<String>) -> Option<String> {
    if enabled { value } else { None }
}

/// Valor da variável do harness, ou `None` se ausente ou se o build não habilita o harness.
pub fn var(name: &str) -> Option<String> {
    gate(ENABLED, std::env::var(name).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_harness_ignores_values() {
        assert_eq!(gate(false, Some("1280x800".into())), None);
        assert_eq!(gate(true, Some("1280x800".into())), Some("1280x800".into()));
        assert_eq!(gate(true, None), None);
    }

    #[test]
    fn missing_variable_is_none() {
        assert_eq!(var("DBM_DOES_NOT_EXIST_FOR_TEST"), None);
    }
}
