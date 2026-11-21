//! User session state (last opened folder, open tabs, active file, pane, editor, viewports),
//! persisted as JSON in the user's config directory (`session.json`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Atomically writes `content` to `path` via a unique temporary file and rename.
pub fn atomic_write_sync(path: &Path, content: &str) -> std::io::Result<()> {
    atomic_write_bytes(path, content.as_bytes())
}

/// Como [`atomic_write_sync`], para dados binários (exports PNG/JPEG). Delega à crate
/// compartilhada `atomic-write` (preserva symlinks, permissões e faz `fsync`).
pub fn atomic_write_bytes(path: &Path, content: &[u8]) -> std::io::Result<()> {
    atomic_write::atomic_write(path, content)
}

/// Sidecar de layout ao lado do arquivo: `<arquivo>.dbml.layout.json` (mesmo da CLI web e VS Code).
fn sidecar_path(dbml: &Path) -> PathBuf {
    let mut s = dbml.as_os_str().to_owned();
    s.push(".layout.json");
    PathBuf::from(s)
}

/// Leitura-modificação-gravação do sidecar (`edges` na UI, `tables` em segundo plano) não pode
/// se intercalar dentro do processo.
static SIDECAR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Posições do sidecar (`tables: { nome: {x, y} }`, mesmo formato da CLI web e do VS Code; enums
/// usam a chave `enum:<nome>`). Ausente ou ilegível = vazio, e o auto-layout decide.
pub fn load_positions(dbml: &Path) -> std::collections::HashMap<String, (f32, f32)> {
    let Some(v) = std::fs::read_to_string(sidecar_path(dbml))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return std::collections::HashMap::new();
    };
    v.get("tables")
        .and_then(|t| t.as_object())
        .map(|tables| {
            tables
                .iter()
                .filter_map(|(k, p)| {
                    Some((
                        k.clone(),
                        (p.get("x")?.as_f64()? as f32, p.get("y")?.as_f64()? as f32),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Grava `tables` no sidecar preservando as demais chaves (`edges`, `viewport`, `colors`...) e
/// as entradas de tabelas que não estão em `pos`. Sidecar existente que não é um objeto JSON não
/// é sobrescrito (erro). Escrita atômica.
pub fn save_positions(
    dbml: &Path,
    pos: &std::collections::HashMap<String, (f32, f32)>,
) -> std::io::Result<()> {
    let _guard = SIDECAR_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let p = sidecar_path(dbml);
    let mut v = match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).map_err(std::io::Error::other)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if pos.is_empty() {
                return Ok(());
            }
            serde_json::Value::Object(serde_json::Map::new())
        }
        Err(e) => return Err(e),
    };
    let obj = v.as_object_mut().ok_or_else(|| {
        std::io::Error::other(crate::i18n::t("sidecar de layout não é um objeto JSON"))
    })?;
    let tables = obj
        .entry("tables")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !tables.is_object() {
        *tables = serde_json::Value::Object(serde_json::Map::new());
    }
    let Some(tables) = tables.as_object_mut() else {
        return Err(std::io::Error::other(crate::i18n::t(
            "sidecar de layout não é um objeto JSON",
        )));
    };
    let round = |n: f32| (f64::from(n) * 100.0).round() / 100.0;
    for (name, &(x, y)) in pos {
        let entry = tables
            .entry(name.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !entry.is_object() {
            *entry = serde_json::Value::Object(serde_json::Map::new());
        }
        if let Some(entry) = entry.as_object_mut() {
            entry.insert("x".into(), serde_json::json!(round(x)));
            entry.insert("y".into(), serde_json::json!(round(y)));
        }
    }
    let json = serde_json::to_string_pretty(&v).map_err(std::io::Error::other)?;
    atomic_write_sync(&p, &json)
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
    let _guard = SIDECAR_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    let obj = v.as_object_mut().ok_or_else(|| {
        std::io::Error::other(crate::i18n::t("sidecar de layout não é um objeto JSON"))
    })?;
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

/// Pasta de configuração do app (`~/.config/db-modeling`, `%APPDATA%\db-modeling`...).
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("db-modeling"))
}

fn path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("session.json"))
}

pub use crate::constants::MAX_VIEWPORTS;

impl SessionState {
    /// Remove entradas de `viewports` cujos arquivos não existem mais no disco e aplica o teto
    /// [`MAX_VIEWPORTS`], preservando abas abertas e arquivo ativo.
    pub fn prune_missing(&mut self) {
        self.viewports.retain(|path, _| path.exists());
        if self.viewports.len() > MAX_VIEWPORTS {
            let keep: std::collections::HashSet<&PathBuf> = self
                .open_tabs
                .iter()
                .chain(self.active_file.iter())
                .chain(self.recent.iter().map(|r| &r.path))
                .collect();
            let to_drop = self.viewports.len() - MAX_VIEWPORTS;
            let removable: Vec<PathBuf> = self
                .viewports
                .keys()
                .filter(|k| !keep.contains(k))
                .take(to_drop)
                .cloned()
                .collect();
            for k in removable {
                self.viewports.remove(&k);
            }
        }
    }
}

pub fn load() -> SessionState {
    let mut state = path().map(|p| load_from(&p)).unwrap_or_default();
    state.prune_missing();
    state
}

/// Avisos de persistência (falha ao gravar, arquivo ilegível) para o shell mostrar na janela.
static NOTICES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static LAST_NOTICE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Registra um aviso; repetir a mesma mensagem em seguida não empilha.
pub fn notice(message: String) {
    let (Ok(mut last), Ok(mut queue)) = (LAST_NOTICE.lock(), NOTICES.lock()) else {
        return;
    };
    if last.as_deref() != Some(message.as_str()) {
        *last = Some(message.clone());
        queue.push(message);
    }
}

/// Uma gravação deu certo: o próximo erro igual volta a ser avisado.
fn notice_recovered() {
    if let Ok(mut last) = LAST_NOTICE.lock() {
        *last = None;
    }
}

pub fn take_notices() -> Vec<String> {
    NOTICES
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}

/// Copia o arquivo ilegível para `<nome>.bak` antes de ser substituído por padrões e avisa.
pub fn quarantine(p: &Path, reason: &str) {
    let mut bak = p.as_os_str().to_owned();
    bak.push(".bak");
    let bak = PathBuf::from(bak);
    eprintln!(
        "{}: {} ({reason})",
        crate::i18n::t("arquivo de estado ilegível"),
        p.display()
    );
    match std::fs::copy(p, &bak) {
        Ok(_) => notice(
            crate::i18n::t(
                "Não foi possível ler {}. Uma cópia ficou em {} e o app usa os padrões.",
            )
            .replacen("{}", &p.display().to_string(), 1)
            .replacen("{}", &bak.display().to_string(), 1),
        ),
        Err(e) => notice(
            crate::i18n::t("Não foi possível ler {} e a cópia de segurança falhou: {}")
                .replacen("{}", &p.display().to_string(), 1)
                .replacen("{}", &e.to_string(), 1),
        ),
    }
}

pub fn load_from(p: &Path) -> SessionState {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return SessionState::default(),
        Err(e) => {
            quarantine(p, &e.to_string());
            return SessionState::default();
        }
    };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        quarantine(p, &e.to_string());
        SessionState::default()
    })
}

pub fn save(state: &SessionState) {
    let Some(p) = path() else { return };
    let mut pruned = state.clone();
    pruned.prune_missing();
    save_to(&p, &pruned);
}

pub fn save_to(p: &Path, state: &SessionState) {
    let mut payload = state.clone();
    payload.version = 1;
    let Ok(json) = serde_json::to_string_pretty(&payload) else {
        return;
    };
    note_write_result(p, atomic_write_sync(p, &json));
}

/// Falha de gravação de estado (sessão/preferências) vira log + aviso na janela; sucesso rearma o aviso.
pub fn note_write_result(p: &Path, result: std::io::Result<()>) {
    match result {
        Ok(()) => notice_recovered(),
        Err(e) => {
            eprintln!("could not write {}: {e}", p.display());
            notice(
                crate::i18n::t("Não foi possível salvar {}: {}")
                    .replacen("{}", &p.display().to_string(), 1)
                    .replacen("{}", &e.to_string(), 1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_permissions_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("dbm-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.dbml");
        let link = dir.join("link.dbml");
        std::fs::write(&target, "before").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &link).unwrap();
        super::atomic_write_sync(&link, "after").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "after");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_write_bytes_replaces_content_without_leaving_temp_files() {
        let dir = std::env::temp_dir().join(format!("dbm-atomic-bytes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("export.png");
        std::fs::write(&target, b"old").unwrap();
        atomic_write_bytes(&target, &[0x89, b'P', b'N', b'G', 0, 255]).unwrap();
        assert_eq!(
            std::fs::read(&target).unwrap(),
            [0x89, b'P', b'N', b'G', 0, 255]
        );
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
        let entries = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .count();
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
        assert_eq!(
            v["edges"]["a.id->b.id"],
            serde_json::json!({ "straight": true })
        );
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
    fn positions_roundtrip_in_sidecar_preserving_other_keys() {
        let dir = std::env::temp_dir().join(format!("dbm_positions_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dbml = dir.join("p.dbml");
        let side = dir.join("p.dbml.layout.json");
        assert!(load_positions(&dbml).is_empty());
        // sem sidecar e sem posições: não cria arquivo
        save_positions(&dbml, &std::collections::HashMap::new()).unwrap();
        assert!(!side.exists());
        std::fs::write(
            &side,
            r##"{"version":1,"tables":{"old":{"x":1,"y":2,"color":"#fff"},"users":{"x":0,"y":0}},"viewport":{"zoom":2,"pan_x":1,"pan_y":2},"edges":{"a.id->b.id":{"straight":true}}}"##,
        )
        .unwrap();
        let pos = std::collections::HashMap::from([
            ("users".to_string(), (120.126_f32, -40.0_f32)),
            ("enum:status".to_string(), (10.0, 20.5)),
        ]);
        save_positions(&dbml, &pos).unwrap();
        let loaded = load_positions(&dbml);
        assert_eq!(loaded["users"], (120.13, -40.0));
        assert_eq!(loaded["enum:status"], (10.0, 20.5));
        // tabela que não está em `pos` permanece, com as demais propriedades
        assert_eq!(loaded["old"], (1.0, 2.0));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
        assert_eq!(v["tables"]["old"]["color"], "#fff");
        assert_eq!(v["viewport"]["zoom"], 2);
        assert_eq!(v["edges"]["a.id->b.id"]["straight"], true);
        // sidecar corrompido não é sobrescrito
        std::fs::write(&side, "{oops").unwrap();
        assert!(save_positions(&dbml, &pos).is_err());
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
        // o arquivo ilegível é preservado em .bak antes de ser substituído
        assert_eq!(
            std::fs::read_to_string(dir.join("session.json.bak")).unwrap(),
            "{not valid json"
        );
        assert!(
            take_notices()
                .iter()
                .any(|n| n.contains("session.json.bak"))
        );
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

    #[test]
    fn prune_missing_removes_deleted_viewport_files() {
        let dir = std::env::temp_dir().join(format!("dbm_prune_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let existing = dir.join("exists.dbml");
        let missing = dir.join("deleted.dbml");
        std::fs::write(&existing, "Table a { id int }").unwrap();
        let vp = ViewportState {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let mut state = SessionState::default();
        state.viewports.insert(existing.clone(), vp.clone());
        state.viewports.insert(missing.clone(), vp);
        state.prune_missing();
        assert!(state.viewports.contains_key(&existing));
        assert!(!state.viewports.contains_key(&missing));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
