//! Recuperação após queda: cópias das abas com edições não salvas, gravadas fora da pasta do
//! usuário (`<config>/recovery/<id>.json`), com debounce, e também pelo panic hook antes do abort
//! (`panic = "abort"` no release). Na próxima abertura o shell devolve as abas como não salvas.
//! O arquivo some assim que a aba é salva ou fechada.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Aba com edições não salvas, como o shell a publica.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub text: std::sync::Arc<str>,
}

/// Formato em disco.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub text: String,
}

impl From<&Live> for Entry {
    fn from(l: &Live) -> Self {
        Self {
            id: l.id,
            title: l.title.clone(),
            path: l.path.clone(),
            text: l.text.to_string(),
        }
    }
}

static LIVE: Mutex<Vec<Live>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// Id único entre execuções (milissegundos + contador), usado no nome do arquivo de recuperação.
pub fn next_id() -> u64 {
    let base = crate::session::now_secs() * 1000;
    let mut cur = NEXT_ID.load(Ordering::Relaxed);
    loop {
        let next = cur.max(base) + 1;
        match NEXT_ID.compare_exchange(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => cur = actual,
        }
    }
}

pub fn dir() -> Option<PathBuf> {
    crate::session::config_dir().map(|d| d.join("recovery"))
}

pub fn set_live(docs: Vec<Live>) {
    if let Ok(mut live) = LIVE.lock() {
        *live = docs;
    }
}

fn entry_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Grava uma cópia por aba e apaga as cópias de abas que já não precisam de recuperação.
pub fn write_all(dir: &Path, docs: &[Live]) -> std::io::Result<()> {
    dump(dir, docs)?;
    let keep: std::collections::HashSet<_> = docs.iter().map(|d| entry_path(dir, d.id)).collect();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "json") && !keep.contains(&p) {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}

/// Grava sem apagar nada (usado pelo panic hook).
pub fn dump(dir: &Path, docs: &[Live]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for doc in docs {
        let json = serde_json::to_string(&Entry::from(doc)).map_err(std::io::Error::other)?;
        crate::session::atomic_write_sync(&entry_path(dir, doc.id), &json)?;
    }
    Ok(())
}

/// Panic hook: não bloqueia se o panic ocorreu com o lock tomado.
pub fn dump_live_for_panic() {
    let Some(dir) = dir() else { return };
    if let Ok(live) = LIVE.try_lock()
        && !live.is_empty()
    {
        let _ = dump(&dir, &live);
    }
}

/// Cópias deixadas por uma execução anterior, em ordem de criação. Arquivos ilegíveis ficam
/// de fora (e no disco, para inspeção).
pub fn read_all(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    out.sort_by_key(|e| e.id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(id: u64, text: &str) -> Live {
        Live {
            id,
            title: format!("t{id}.dbml"),
            path: Some(PathBuf::from("/x/a.dbml")),
            text: text.into(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm-recovery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let (a, b) = (next_id(), next_id());
        assert!(b > a);
    }

    #[test]
    fn write_read_roundtrip_and_prune() {
        let dir = temp("rt");
        write_all(&dir, &[live(2, "Table b {}"), live(1, "Table a {}")]).unwrap();
        let read = read_all(&dir);
        assert_eq!(read.iter().map(|e| e.id).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(read[0].text, "Table a {}");
        assert_eq!(read[1].path, Some(PathBuf::from("/x/a.dbml")));
        // aba 1 foi salva: a cópia some, a 2 permanece
        write_all(&dir, &[live(2, "Table b {}")]).unwrap();
        assert_eq!(read_all(&dir).iter().map(|e| e.id).collect::<Vec<_>>(), [2]);
        write_all(&dir, &[]).unwrap();
        assert_eq!(read_all(&dir), Vec::<Entry>::new());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dump_keeps_other_copies_and_bad_files_are_skipped() {
        let dir = temp("dump");
        dump(&dir, &[live(1, "a")]).unwrap();
        dump(&dir, &[live(2, "b")]).unwrap();
        std::fs::write(dir.join("9.json"), "{ruim").unwrap();
        assert_eq!(
            read_all(&dir).iter().map(|e| e.id).collect::<Vec<_>>(),
            [1, 2]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_dir_reads_empty() {
        assert_eq!(
            read_all(Path::new("/nonexistent/dbm-recovery")),
            Vec::<Entry>::new()
        );
    }
}
