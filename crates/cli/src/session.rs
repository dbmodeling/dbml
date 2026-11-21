use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CliSession {
    pub last_target: Option<PathBuf>,
    pub active_file: Option<PathBuf>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("db-modeling").join("cli-session.json"))
}

#[must_use]
pub fn load() -> CliSession {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

/// Versão assíncrona de [`load`]: a leitura roda no pool de threads bloqueantes.
pub async fn load_async() -> CliSession {
    tokio::task::spawn_blocking(load).await.unwrap_or_default()
}

/// Versão assíncrona de [`save`]: a gravação roda no pool de threads bloqueantes.
pub async fn save_async(session: CliSession) {
    let _ = tokio::task::spawn_blocking(move || save(&session)).await;
}

#[must_use]
pub fn load_from(p: &Path) -> CliSession {
    let Ok(text) = std::fs::read_to_string(p) else {
        return CliSession::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(session: &CliSession) {
    let Some(p) = path() else { return };
    save_to(&p, session);
}

pub fn save_to(p: &Path, session: &CliSession) {
    if let Ok(json) = serde_json::to_string_pretty(session) {
        let _ = atomic_write::atomic_write(p, json.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_cli_session() {
        let dir = std::env::temp_dir().join(format!("dbm_cli_sess_{}", std::process::id()));
        let file = dir.join("cli-session.json");
        let s = CliSession {
            last_target: Some(PathBuf::from("/tmp/project")),
            active_file: Some(PathBuf::from("/tmp/project/schema.dbml")),
        };
        save_to(&file, &s);
        assert_eq!(load_from(&file), s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
