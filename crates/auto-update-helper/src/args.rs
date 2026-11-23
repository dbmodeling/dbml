//! Argumentos do helper: `--launch true|false`, `--wait-pid <pid>` e o resto repassado ao app.
//!
//! Como no Zed, `--launch` aceita `--launch false` e `--launch=false`; qualquer outro argumento
//! vira argumento do app relançado. O padrão é `launch = true`.

use std::ffi::OsString;
use std::fmt;

use crate::constants::{ARG_LAUNCH, ARG_VALUE_SEPARATOR, ARG_WAIT_PID, LAUNCH_FALSE, LAUNCH_TRUE};

/// Opções interpretadas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Abrir o app depois da troca (`--launch true`).
    pub launch: bool,
    /// Esperar este PID sair antes de trocar (`--wait-pid`).
    pub wait_pid: Option<u32>,
    /// Argumentos repassados ao app relançado.
    pub launch_args: Vec<OsString>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            launch: true,
            wait_pid: None,
            launch_args: Vec::new(),
        }
    }
}

/// Argumento inválido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgError {
    /// `--launch` sem valor.
    MissingLaunchValue,
    /// `--launch` com valor que não é `true` nem `false`.
    InvalidLaunchValue(String),
    /// `--wait-pid` sem valor.
    MissingPid,
    /// `--wait-pid` com valor que não é um PID (inteiro positivo).
    InvalidPid(String),
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLaunchValue => write!(f, "{ARG_LAUNCH} sem valor"),
            Self::InvalidLaunchValue(value) => {
                write!(f, "valor inválido para {ARG_LAUNCH}: `{value}`")
            }
            Self::MissingPid => write!(f, "{ARG_WAIT_PID} sem valor"),
            Self::InvalidPid(value) => write!(f, "PID inválido para {ARG_WAIT_PID}: `{value}`"),
        }
    }
}

impl std::error::Error for ArgError {}

fn parse_launch(value: &str) -> Result<bool, ArgError> {
    match value {
        LAUNCH_TRUE => Ok(true),
        LAUNCH_FALSE => Ok(false),
        other => Err(ArgError::InvalidLaunchValue(other.to_owned())),
    }
}

fn parse_pid(value: &str) -> Result<u32, ArgError> {
    value
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid != 0)
        .ok_or_else(|| ArgError::InvalidPid(value.to_owned()))
}

/// Separa `--chave=valor`: devolve o valor se `arg` for exatamente `key=...`.
fn inline_value<'a>(arg: &'a str, key: &str) -> Option<&'a str> {
    arg.strip_prefix(key)?.strip_prefix(ARG_VALUE_SEPARATOR)
}

/// Interpreta os argumentos **sem** o nome do programa.
///
/// # Errors
/// [`ArgError`] se `--launch` ou `--wait-pid` vierem sem valor ou com valor inválido.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options, ArgError> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let text = arg.to_str();
        if text == Some(ARG_LAUNCH) {
            let value = args.next().ok_or(ArgError::MissingLaunchValue)?;
            options.launch = parse_launch(&value.to_string_lossy())?;
        } else if let Some(value) = text.and_then(|t| inline_value(t, ARG_LAUNCH)) {
            options.launch = parse_launch(value)?;
        } else if text == Some(ARG_WAIT_PID) {
            let value = args.next().ok_or(ArgError::MissingPid)?;
            options.wait_pid = Some(parse_pid(&value.to_string_lossy())?);
        } else if let Some(value) = text.and_then(|t| inline_value(t, ARG_WAIT_PID)) {
            options.wait_pid = Some(parse_pid(value)?);
        } else {
            options.launch_args.push(arg);
        }
    }
    Ok(options)
}

/// Argumentos que o app passa ao helper (`--launch <bool>` e, se houver, `--wait-pid <pid>`).
#[must_use]
pub fn build(launch: bool, wait_pid: Option<u32>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from(ARG_LAUNCH),
        OsString::from(if launch { LAUNCH_TRUE } else { LAUNCH_FALSE }),
    ];
    if let Some(pid) = wait_pid {
        args.push(OsString::from(ARG_WAIT_PID));
        args.push(OsString::from(pid.to_string()));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn sem_argumentos_lanca_por_padrao() {
        assert_eq!(parse(os(&[])), Ok(Options::default()));
        assert!(Options::default().launch);
    }

    #[test]
    fn launch_aceita_as_duas_formas() {
        assert!(!parse(os(&["--launch", "false"])).unwrap().launch);
        assert!(!parse(os(&["--launch=false"])).unwrap().launch);
        assert!(parse(os(&["--launch=true"])).unwrap().launch);
    }

    #[test]
    fn launch_invalido_ou_sem_valor_e_erro() {
        assert_eq!(parse(os(&["--launch"])), Err(ArgError::MissingLaunchValue));
        assert_eq!(
            parse(os(&["--launch", "talvez"])),
            Err(ArgError::InvalidLaunchValue("talvez".into()))
        );
    }

    #[test]
    fn wait_pid_valido_e_invalido() {
        assert_eq!(parse(os(&["--wait-pid", "42"])).unwrap().wait_pid, Some(42));
        assert_eq!(parse(os(&["--wait-pid=7"])).unwrap().wait_pid, Some(7));
        for bad in [
            ["--wait-pid", "0"],
            ["--wait-pid", "-1"],
            ["--wait-pid", "x"],
        ] {
            assert!(
                matches!(parse(os(&bad)), Err(ArgError::InvalidPid(_))),
                "{bad:?}"
            );
        }
        assert_eq!(parse(os(&["--wait-pid"])), Err(ArgError::MissingPid));
    }

    #[test]
    fn outros_argumentos_vao_para_o_app() {
        let options = parse(os(&[
            "--launch",
            "true",
            "--user-data-dir",
            "C:\\Meus Dados",
            "schema.dbml",
        ]))
        .unwrap();
        assert!(options.launch);
        assert_eq!(
            options.launch_args,
            os(&["--user-data-dir", "C:\\Meus Dados", "schema.dbml"])
        );
    }

    #[test]
    fn build_e_parse_sao_inversos() {
        for (launch, pid) in [
            (true, None),
            (false, None),
            (true, Some(99)),
            (false, Some(1)),
        ] {
            let options = parse(build(launch, pid)).unwrap();
            assert_eq!((options.launch, options.wait_pid), (launch, pid));
            assert!(options.launch_args.is_empty());
        }
    }
}
