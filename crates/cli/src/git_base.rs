//! Versão base do modo Comparar: o conteúdo do arquivo aberto no `HEAD` do repositório git.
//!
//! O servidor só lê o arquivo que ele mesmo serve (nunca um caminho vindo do cliente) e só
//! executa `git` com argumentos fixos; o nome do arquivo vai como `HEAD:./<nome>`, nunca como
//! opção.

use crate::constants::{
    GIT_BASE_ERR_GIT_UNAVAILABLE, GIT_BASE_ERR_NO_COMMITS, GIT_BASE_ERR_NOT_A_REPO,
    GIT_BASE_ERR_NOT_UTF8, GIT_BASE_ERR_UNTRACKED, GIT_PROGRAM, GIT_REPO_ENV_VARS,
};
use std::path::Path;
use std::process::{Command, Stdio};

/// Por que não foi possível obter o conteúdo do `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitBaseError {
    /// O executável `git` não está instalado ou não está no `PATH`.
    GitUnavailable,
    /// O arquivo não está dentro de um repositório git.
    NotARepo,
    /// O repositório ainda não tem nenhum commit.
    NoCommits,
    /// O arquivo não existe no `HEAD` (novo, ainda não versionado).
    Untracked,
    /// O blob do `HEAD` não é UTF-8.
    NotUtf8,
}

impl GitBaseError {
    /// Código estável do protocolo (o cliente traduz a mensagem a partir dele).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::GitUnavailable => GIT_BASE_ERR_GIT_UNAVAILABLE,
            Self::NotARepo => GIT_BASE_ERR_NOT_A_REPO,
            Self::NoCommits => GIT_BASE_ERR_NO_COMMITS,
            Self::Untracked => GIT_BASE_ERR_UNTRACKED,
            Self::NotUtf8 => GIT_BASE_ERR_NOT_UTF8,
        }
    }
}

impl std::fmt::Display for GitBaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::GitUnavailable => "git is not installed or not on PATH",
            Self::NotARepo => "the file is not inside a git repository",
            Self::NoCommits => "the repository has no commits yet",
            Self::Untracked => "the file is not tracked in git HEAD (new file)",
            Self::NotUtf8 => "the file in git HEAD is not valid UTF-8",
        })
    }
}

impl std::error::Error for GitBaseError {}

/// Roda `git -C <dir> <args>`; `Err(GitUnavailable)` se o executável não existe.
/// `git -C <dir>` isolado de variáveis de repositório herdadas (ex.: de hooks do git).
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new(GIT_PROGRAM);
    for var in GIT_REPO_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd.arg("-C").arg(dir);
    cmd
}

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output, GitBaseError> {
    git_command(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| GitBaseError::GitUnavailable)
}

/// Conteúdo de `file` no `HEAD` do repositório que o contém (`git cat-file blob HEAD:./<nome>`).
///
/// # Errors
///
/// Veja [`GitBaseError`]: sem `git`, fora de repositório, sem commits, arquivo não versionado ou
/// conteúdo que não é UTF-8.
pub fn read_head_blob(file: &Path) -> Result<String, GitBaseError> {
    let dir = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(GitBaseError::Untracked)?;

    let inside = git(dir, &["rev-parse", "--is-inside-work-tree"])?;
    if !inside.status.success() || inside.stdout.trim_ascii() != b"true" {
        return Err(GitBaseError::NotARepo);
    }
    if !git(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?
        .status
        .success()
    {
        return Err(GitBaseError::NoCommits);
    }
    let blob = git(dir, &["cat-file", "blob", &format!("HEAD:./{name}")])?;
    if !blob.status.success() {
        return Err(GitBaseError::Untracked);
    }
    String::from_utf8(blob.stdout).map_err(|_| GitBaseError::NotUtf8)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("dbml_git_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn run(dir: &Path, args: &[&str]) {
        let status = git_command(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c"])
            .arg("commit.gpgsign=false")
            .args(args)
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn init_repo(dir: &Path) {
        run(dir, &["init", "-q"]);
    }

    #[test]
    fn outside_a_repo_is_not_a_repo() {
        let t = TempDir::new("norepo");
        let file = t.0.join("a.dbml");
        std::fs::write(&file, "Table a { id int }\n").unwrap();
        let out = git_command(&t.0)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .unwrap();
        if out.status.success() {
            return; // temp_dir dentro de um repositório: cenário não reproduzível aqui
        }
        assert_eq!(read_head_blob(&file), Err(GitBaseError::NotARepo));
    }

    #[test]
    fn repo_without_commits() {
        let t = TempDir::new("nocommits");
        init_repo(&t.0);
        let file = t.0.join("a.dbml");
        std::fs::write(&file, "Table a { id int }\n").unwrap();
        assert_eq!(read_head_blob(&file), Err(GitBaseError::NoCommits));
    }

    #[test]
    fn untracked_file_is_reported() {
        let t = TempDir::new("untracked");
        init_repo(&t.0);
        std::fs::write(t.0.join("old.dbml"), "Table o { id int }\n").unwrap();
        run(&t.0, &["add", "old.dbml"]);
        run(&t.0, &["commit", "-q", "-m", "init"]);
        let new = t.0.join("new.dbml");
        std::fs::write(&new, "Table n { id int }\n").unwrap();
        assert_eq!(read_head_blob(&new), Err(GitBaseError::Untracked));
    }

    #[test]
    fn returns_head_content_not_working_tree() {
        let t = TempDir::new("head");
        init_repo(&t.0);
        let sub = t.0.join("sub dir");
        std::fs::create_dir_all(&sub).unwrap();
        let file = sub.join("schema.dbml");
        std::fs::write(&file, "Table a { id int }\n").unwrap();
        run(&t.0, &["add", "."]);
        run(&t.0, &["commit", "-q", "-m", "init"]);
        std::fs::write(&file, "Table a { id int\n  name text }\n").unwrap();
        assert_eq!(read_head_blob(&file).unwrap(), "Table a { id int }\n");
    }

    #[test]
    fn non_utf8_blob_is_reported() {
        let t = TempDir::new("utf8");
        init_repo(&t.0);
        let file = t.0.join("a.dbml");
        std::fs::write(&file, [0xff, 0xfe, 0x41]).unwrap();
        run(&t.0, &["add", "."]);
        run(&t.0, &["commit", "-q", "-m", "init"]);
        assert_eq!(read_head_blob(&file), Err(GitBaseError::NotUtf8));
    }

    #[test]
    fn error_codes_are_stable_and_distinct() {
        let all = [
            GitBaseError::GitUnavailable,
            GitBaseError::NotARepo,
            GitBaseError::NoCommits,
            GitBaseError::Untracked,
            GitBaseError::NotUtf8,
        ];
        let mut codes: Vec<_> = all.iter().map(|e| e.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
        assert_eq!(GitBaseError::Untracked.code(), "untracked");
    }
}
