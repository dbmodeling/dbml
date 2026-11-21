//! User session state (last opened folder, open tabs, active file, pane, editor, viewports),
//! persisted as JSON in the user's config directory (`session.json`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Atomically writes `content` to `path` via a unique temporary file and rename.
pub fn atomic_write_sync(path: &Path, content: &str) -> std::io::Result<()> {
    atomic_write_bytes(path, content.as_bytes())
}

/// Como [`atomic_write_sync`], para dados binários (exports PNG/JPEG). Faz `fsync` do arquivo
/// temporário antes do rename e, em Unix, do diretório depois, para que queda de energia não
/// deixe o destino vazio ou parcial.
pub fn atomic_write_bytes(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let target = if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));
    let permissions = std::fs::metadata(&target).ok().map(|m| m.permissions());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if permissions.is_some() { 0o600 } else { 0o666 });
    }
    let mut file = options.open(&tmp_path)?;
    let result = (|| {
        file.write_all(content)?;
        if let Some(permissions) = permissions { file.set_permissions(permissions)?; }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, &target)
    })();
    if result.is_err() { let _ = std::fs::remove_file(&tmp_path); }
    result?;
    // Persiste a entrada de diretório do rename (não existe em Windows; melhor esforço).
    #[cfg(unix)]
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Sidecar de layout ao lado do arquivo: `<arquivo>.dbml.layout.json` (mesmo da CLI web e VS Code).
fn sidecar_path(dbml: &Path) -> PathBuf {
    let mut s = dbml.as_os_str().to_owned();
    s.push(".layout.json");
    PathBuf::from(s)
}

/// `edges` do sidecar; ausente ou ilegível = sem estilos.
pub fn load_edges(dbml: &Path) -> diagram_export::EdgeStyles {
    std::fs::read_to_string(sidecar_path(dbml))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|mut v| serde_json::from_value(v.get_mut("edges")?.take()).ok())
        .unwrap_or_default()
}

/// Grava `edges` no sidecar preservando as demais chaves (`tables`, `viewport`…). Sidecar
/// existente que não é um objeto JSON não é sobrescrito (erro), para não perder dados.
pub fn save_edges(dbml: &Path, edges: &diagram_export::EdgeStyles) -> std::io::Result<()> {
    let p = sidecar_path(dbml);
    let mut v = match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).map_err(std::io::Error::other)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if edges.is_empty() {
                return Ok(());
            }
            serde_json::Value::Object(serde_json::Map::new())
        }
        Err(e) => return Err(e),
    };
    let obj = v
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other(crate::i18n::t("sidecar de layout não é um objeto JSON")))?;
    let sorted: BTreeMap<_, _> = edges.iter().collect();
    obj.insert(
        "edges".into(),
        serde_json::to_value(sorted).map_err(std::io::Error::other)?,
    );
    let json = serde_json::to_string_pretty(&v).map_err(std::io::Error::other)?;
    atomic_write_sync(&p, &json)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewportState {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

/// Arquivo `.dbml` aberto/salvo recentemente (tela inicial).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: PathBuf,
    pub tables: usize,
    /// instante em segundos desde a época Unix
    pub at: u64,
}

const MAX_RECENT: usize = 8;
const MONTHS: [&str; 12] = [
    "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
];

/// Põe `entry` no topo, sem duplicatas, respeitando o limite.
pub fn push_recent(list: &mut Vec<RecentFile>, entry: RecentFile) {
    list.retain(|r| r.path != entry.path);
    list.insert(0, entry);
    list.truncate(MAX_RECENT);
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// "agora", "há 5 min", "há 3 h", "ontem" ou data curta ("mar 5", em UTC).
pub fn ago(at: u64, now: u64) -> String {
    let s = now.saturating_sub(at);
    match s {
        0..60 => crate::i18n::t("agora").into(),
        60..3600 => crate::i18n::t("há {} min").replace("{}", &(s / 60).to_string()),
        3600..86_400 => crate::i18n::t("há {} h").replace("{}", &(s / 3600).to_string()),
        86_400..172_800 => crate::i18n::t("ontem").into(),
        _ => {
            // dias desde a época -> data civil (algoritmo de Howard Hinnant)
            let z = (at / 86_400) as i64 + 719_468;
            let doe = z.rem_euclid(146_097);
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let day = doy - (153 * mp + 2) / 5 + 1;
            let month = if mp < 10 { mp + 3 } else { mp - 9 };
            format!("{} {day}", crate::i18n::t(MONTHS[(month - 1) as usize]))
        }
    }
}

/// Pasta para exibição, com `~` no lugar do home.
pub fn short_dir(dir: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
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
    /// visão "Code" (só o editor); ausente em sessões antigas
    pub code_view: Option<bool>,
    pub recent: Vec<RecentFile>,
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

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_permissions_and_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("dbm-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.dbml");
        let link = dir.join("link.dbml");
        std::fs::write(&target, "before").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &link).unwrap();
        super::atomic_write_sync(&link, "after").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "after");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_write_bytes_replaces_content_without_leaving_temp_files() {
        let dir = std::env::temp_dir().join(format!("dbm-atomic-bytes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("export.png");
        std::fs::write(&target, b"old").unwrap();
        atomic_write_bytes(&target, &[0x89, b'P', b'N', b'G', 0, 255]).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), [0x89, b'P', b'N', b'G', 0, 255]);
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temporários sobrando: {leftovers:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_write_failure_keeps_original_and_cleans_temp() {
        let dir = std::env::temp_dir().join(format!("dbm-atomic-fail-{}", std::process::id()));
        let target = dir.join("dest");
        std::fs::create_dir_all(target.join("child")).unwrap();
        // rename de arquivo sobre diretório não vazio falha: nada pode ficar para trás
        assert!(atomic_write_sync(&target, "x").is_err());
        assert!(target.join("child").is_dir());
        let entries = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).count();
        assert_eq!(entries, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
            code_view: Some(false),
            recent: vec![RecentFile {
                path: PathBuf::from("/tmp/a.dbml"),
                tables: 3,
                at: 1_700_000_000,
            }],
            viewports,
        };
        save_to(&file, &state);
        let loaded = load_from(&file);
        assert_eq!(loaded, state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edges_roundtrip_in_sidecar_preserving_other_keys() {
        let dir = std::env::temp_dir().join(format!("dbm_sidecar_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dbml = dir.join("s.dbml");
        let side = dir.join("s.dbml.layout.json");
        let _ = std::fs::remove_file(&side);
        // sem arquivo e sem estilos: não cria sidecar vazio
        save_edges(&dbml, &diagram_export::EdgeStyles::new()).unwrap();
        assert!(!side.exists());
        std::fs::write(&side, r#"{"tables":{"a":{"x":1,"y":2}},"extra":true}"#).unwrap();
        let edges = diagram_export::EdgeStyles::from([
            (
                "a.id->b.id".to_string(),
                diagram_export::EdgeStyle {
                    straight: true,
                    ..Default::default()
                },
            ),
            (
                "b.x->a.id".to_string(),
                diagram_export::EdgeStyle {
                    straight: false,
                    dx: 40.0,
                    dy: -25.0,
                },
            ),
        ]);
        save_edges(&dbml, &edges).unwrap();
        assert_eq!(load_edges(&dbml), edges);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
        assert_eq!(v["tables"]["a"]["x"], 1);
        assert_eq!(v["extra"], true);
        assert_eq!(v["edges"]["a.id->b.id"], serde_json::json!({ "straight": true }));
        assert_eq!(
            v["edges"]["b.x->a.id"],
            serde_json::json!({ "dx": 40.0, "dy": -25.0 })
        );
        // sidecar corrompido não é sobrescrito
        std::fs::write(&side, "{oops").unwrap();
        assert!(save_edges(&dbml, &edges).is_err());
        assert_eq!(std::fs::read_to_string(&side).unwrap(), "{oops");
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

    fn recent(name: &str) -> RecentFile {
        RecentFile {
            path: PathBuf::from(name),
            tables: 1,
            at: 0,
        }
    }

    #[test]
    fn recent_dedupes_orders_and_caps() {
        let mut list = Vec::new();
        for i in 0..10 {
            push_recent(&mut list, recent(&format!("/f{i}.dbml")));
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, PathBuf::from("/f9.dbml"));
        // reabrir um antigo move para o topo, sem duplicar
        push_recent(&mut list, recent("/f5.dbml"));
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].path, PathBuf::from("/f5.dbml"));
        assert_eq!(list.iter().filter(|r| r.path == list[0].path).count(), 1);
    }

    #[test]
    fn relative_time_text() {
        let now = 1_000_000_000;
        assert_eq!(ago(now - 10, now), "agora");
        assert_eq!(ago(now - 300, now), "há 5 min");
        assert_eq!(ago(now - 7200, now), "há 2 h");
        assert_eq!(ago(now - 100_000, now), "ontem");
        // 2026-03-05 01:00 UTC
        assert_eq!(ago(20_517 * 86_400 + 3600, now + 20_517 * 86_400), "mar 5");
    }

    #[test]
    fn short_dir_uses_tilde() {
        let home = Path::new("/home/u");
        assert_eq!(short_dir(Path::new("/home/u/p/db"), Some(home)), "~/p/db");
        assert_eq!(short_dir(Path::new("/home/u"), Some(home)), "~");
        assert_eq!(short_dir(Path::new("/srv/db"), Some(home)), "/srv/db");
    }
}
