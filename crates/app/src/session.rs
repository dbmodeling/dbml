//! User session state (last opened folder, open tabs, active file, pane, editor, viewports),
//! persisted as JSON in the user's config directory (`session.json`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Atomically writes `content` to `path` via a unique temporary file and rename.
pub fn atomic_write_sync(path: &Path, content: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));
    std::fs::write(&tmp_path, content)?;
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportState {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub version: u32,
    pub project_dir: Option<PathBuf>,
    pub open_tabs: Vec<PathBuf>,
    pub active_file: Option<PathBuf>,
    pub pane: Option<String>,
    pub editor_open: Option<bool>,
    pub viewports: BTreeMap<PathBuf, ViewportState>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("db-modeling").join("session.json"))
}

pub fn load() -> SessionState {
    path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn load_from(p: &Path) -> SessionState {
    let Ok(text) = std::fs::read_to_string(p) else {
        return SessionState::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(state: &SessionState) {
    let Some(p) = path() else { return };
    save_to(&p, state);
}

pub fn save_to(p: &Path, state: &SessionState) {
    let mut payload = state.clone();
    payload.version = 1;
    if let Ok(json) = serde_json::to_string_pretty(&payload) {
        let _ = atomic_write_sync(p, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_session_state() {
        let dir = std::env::temp_dir().join(format!("dbm_session_test_{}", std::process::id()));
        let file = dir.join("session.json");
        let mut viewports = BTreeMap::new();
        viewports.insert(
            PathBuf::from("/tmp/a.dbml"),
            ViewportState {
                zoom: 1.25,
                pan_x: 40.0,
                pan_y: -15.0,
            },
        );
        let state = SessionState {
            version: 1,
            project_dir: Some(PathBuf::from("/tmp/project")),
            open_tabs: vec![PathBuf::from("/tmp/a.dbml"), PathBuf::from("/tmp/b.dbml")],
            active_file: Some(PathBuf::from("/tmp/b.dbml")),
            pane: Some("files".to_string()),
            editor_open: Some(true),
            viewports,
        };
        save_to(&file, &state);
        let loaded = load_from(&file);
        assert_eq!(loaded, state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupted_file_falls_back_to_default() {
        let missing = std::env::temp_dir().join("dbm_nonexistent_session_12345.json");
        assert_eq!(load_from(&missing), SessionState::default());

        let dir = std::env::temp_dir().join(format!("dbm_corrupt_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let corrupt = dir.join("session.json");
        std::fs::write(&corrupt, "{not valid json").unwrap();
        assert_eq!(load_from(&corrupt), SessionState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
