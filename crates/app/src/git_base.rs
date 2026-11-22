//! Base do modo Comparar (#18): o conteúdo do arquivo aberto no `HEAD` do git
//! (`git show HEAD:./<arquivo>`), lido sem tocar no buffer nem no disco.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::constants::{GIT_BIN, GIT_HEAD_SPEC_PREFIX, GIT_REPO_ENV_VARS};

/// Por que não há base para comparar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadError {
    /// Aba sem arquivo no disco (ainda não salva).
    Unsaved,
    /// `git` não está instalado ou não pôde ser executado.
    GitUnavailable,
    /// A pasta do arquivo não está em um repositório git.
    NotARepo,
    /// O repositório ainda não tem commits.
    NoCommits,
    /// O arquivo não existe no `HEAD` (não rastreado ou novo).
    NotInHead,
}

impl HeadError {
    /// Chave pt-BR da mensagem (traduzida por `i18n::t`).
    pub const fn message(self) -> &'static str {
        match self {
            Self::Unsaved => "Salve o arquivo para comparar com o HEAD do git",
            Self::GitUnavailable => "git não encontrado no PATH",
            Self::NotARepo => "O arquivo não está em um repositório git",
            Self::NoCommits => "O repositório ainda não tem commits",
            Self::NotInHead => "Arquivo novo ou não rastreado: não existe no HEAD",
        }
    }
}

/// Pasta onde rodar o git e o `rev:path` relativo a ela (`HEAD:./schema.dbml`).
/// `None` quando o caminho não tem nome de arquivo.
pub fn head_spec(path: &Path) -> Option<(PathBuf, String)> {
    let name = path.file_name()?.to_str()?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Some((dir, format!("{GIT_HEAD_SPEC_PREFIX}{name}")))
}

/// `git -C <dir>` isolado de variáveis de repositório herdadas do ambiente.
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new(GIT_BIN);
    for var in GIT_REPO_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dir);
    cmd
}

fn git(dir: &Path, args: &[&str]) -> Result<Output, HeadError> {
    let mut cmd = git_command(dir);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(crate::constants::WIN_CREATE_NO_WINDOW);
    }
    cmd.output().map_err(|_| HeadError::GitUnavailable)
}

/// Conteúdo do arquivo no `HEAD` do repositório que o contém.
pub fn head_source(path: Option<&Path>) -> Result<String, HeadError> {
    let (dir, spec) = path.and_then(head_spec).ok_or(HeadError::Unsaved)?;
    if !git(&dir, &["rev-parse", "--is-inside-work-tree"])?
        .status
        .success()
    {
        return Err(HeadError::NotARepo);
    }
    if !git(&dir, &["rev-parse", "--verify", "--quiet", "HEAD"])?
        .status
        .success()
    {
        return Err(HeadError::NoCommits);
    }
    let out = git(&dir, &["show", &spec])?;
    if !out.status.success() {
        return Err(HeadError::NotInHead);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("dbm-git-base-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        let ok = git_command(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} falhou");
    }

    #[test]
    fn head_spec_is_relative_to_parent() {
        let (dir, spec) = head_spec(Path::new("/a/b/schema.dbml")).unwrap();
        assert_eq!(dir, PathBuf::from("/a/b"));
        assert_eq!(spec, "HEAD:./schema.dbml");
        let (dir, _) = head_spec(Path::new("x.dbml")).unwrap();
        assert_eq!(dir, PathBuf::from("."));
        assert!(head_spec(Path::new("/")).is_none());
    }

    #[test]
    fn unsaved_doc_has_no_base() {
        assert_eq!(head_source(None), Err(HeadError::Unsaved));
    }

    #[test]
    fn reports_not_a_repo_no_commits_untracked_and_reads_head() {
        let plain = scratch("plain");
        let file = plain.join("s.dbml");
        fs::write(&file, "Table a {}\n").unwrap();
        // o temp pode estar dentro de um repo em algum ambiente: aí o arquivo é só não rastreado
        assert!(matches!(
            head_source(Some(&file)),
            Err(HeadError::NotARepo | HeadError::NotInHead)
        ));

        let repo = scratch("repo");
        run(&repo, &["init", "-q"]);
        let file = repo.join("s.dbml");
        fs::write(&file, "Table a {\n  id int\n}\n").unwrap();
        assert_eq!(head_source(Some(&file)), Err(HeadError::NoCommits));

        run(&repo, &["add", "s.dbml"]);
        run(&repo, &["commit", "-q", "-m", "c"]);
        // edição local não altera a base
        fs::write(&file, "Table b {}\n").unwrap();
        assert_eq!(
            head_source(Some(&file)).as_deref(),
            Ok("Table a {\n  id int\n}\n")
        );

        let new_file = repo.join("novo.dbml");
        fs::write(&new_file, "Table n {}\n").unwrap();
        assert_eq!(head_source(Some(&new_file)), Err(HeadError::NotInHead));

        let _ = fs::remove_dir_all(&plain);
        let _ = fs::remove_dir_all(&repo);
    }
}
