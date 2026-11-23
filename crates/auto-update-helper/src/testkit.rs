//! Apoio aos testes: instalação falsa em pasta temporária e sistema de arquivos com falhas.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::constants::MANAGED_FILES;
use crate::fsops::{Fs, RealFs};
use crate::jobs::Job;
use crate::layout::Layout;

/// Versão gravada na flag das instalações falsas.
pub const FLAG_VERSION: &str = "0.4.1";

/// Instalação com os 4 arquivos antigos no lugar e os 4 novos em `install\`, com a flag.
pub struct Fixture {
    _dir: TempDir,
    pub layout: Layout,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        for file in MANAGED_FILES {
            let rel = file.join("/");
            write(&layout.live(file), &format!("old:{rel}"));
            write(&layout.staged(file), &format!("new:{rel}"));
        }
        write(&layout.flag_file(), FLAG_VERSION);
        Self { _dir: dir, layout }
    }

    /// Todos os arquivos sob `{app}` (caminho relativo com `/` → conteúdo).
    pub fn snapshot(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        walk(self.layout.app_dir(), self.layout.app_dir(), &mut map);
        map
    }

    /// Estado esperado depois de uma troca completa.
    pub fn expected_after_success() -> BTreeMap<String, String> {
        MANAGED_FILES
            .iter()
            .map(|file| {
                let rel = file.join("/");
                (rel.clone(), format!("new:{rel}"))
            })
            .collect()
    }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn walk(root: &Path, dir: &Path, map: &mut BTreeMap<String, String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, map);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            map.insert(rel, fs::read_to_string(&path).unwrap());
        }
    }
}

/// Operação que falha **sempre** (falha permanente: esgota o prazo de repetição).
#[derive(Debug, Clone)]
pub enum Fault {
    MakeDir(PathBuf),
    RenameFrom(PathBuf),
    RenameFromTo(PathBuf, PathBuf),
    RemoveAll(PathBuf),
    InUse(PathBuf),
}

impl Fault {
    /// A falha que derruba exatamente o `apply` deste job.
    pub fn of(job: &Job) -> Self {
        match job {
            Job::MakeDir(path) => Self::MakeDir(path.clone()),
            Job::Move { from, to, .. } => Self::RenameFromTo(from.clone(), to.clone()),
            Job::RemoveDir(path) => Self::RemoveAll(path.clone()),
        }
    }
}

/// `RealFs` que falha nas operações listadas.
pub struct FaultyFs {
    faults: Vec<Fault>,
}

impl FaultyFs {
    pub fn failing(fault: Fault) -> Self {
        Self::failing_all(vec![fault])
    }

    pub fn failing_all(faults: Vec<Fault>) -> Self {
        Self { faults }
    }

    fn denied() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }
}

impl Fs for FaultyFs {
    fn exists(&self, path: &Path) -> bool {
        RealFs.exists(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        RealFs.is_dir(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        if self
            .faults
            .iter()
            .any(|f| matches!(f, Fault::MakeDir(p) if p == path))
        {
            return Err(Self::denied());
        }
        RealFs.create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let hit = self.faults.iter().any(|fault| match fault {
            Fault::RenameFrom(f) => f == from,
            Fault::RenameFromTo(f, t) => f == from && t == to,
            _ => false,
        });
        if hit {
            return Err(Self::denied());
        }
        RealFs.rename(from, to)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        RealFs.remove_dir(path)
    }

    fn remove_all(&self, path: &Path) -> io::Result<()> {
        if self
            .faults
            .iter()
            .any(|f| matches!(f, Fault::RemoveAll(p) if p == path))
        {
            return Err(Self::denied());
        }
        RealFs.remove_all(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        RealFs.read_to_string(path)
    }

    fn is_in_use(&self, path: &Path) -> bool {
        self.faults
            .iter()
            .any(|f| matches!(f, Fault::InUse(p) if p == path))
            || RealFs.is_in_use(path)
    }
}
