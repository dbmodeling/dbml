//! Apoio dos testes de integração: servidor HTTP local (`std::net`), chaves de TESTE descartáveis
//! geradas em tempo de teste e um `Harness` que monta uma rodada completa sem rede externa.
//!
//! Nenhuma chave real é lida: o par de assinatura nasce e morre dentro do teste.

#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::SystemTime;

use semver::Version;
use updater::platform::{PlatformKey, TargetOs};
use updater::policy::PolicyEnv;
use updater::state::{Trigger, UpdateState};
use updater::verify::sha256_hex;
use updater_net::etag::MemoryEtagStore;
use updater_net::keys::KeySet;
use updater_net::round::{RoundOutcome, RoundParams, Services, run_round};
use updater_net::ureq_transport::{TransportConfig, UreqTransport};
use url::Url;

/// Caminho do manifesto no servidor de teste (o mesmo do GitHub).
pub const MANIFEST_PATH: &str = "/dbmodeling/dbml/releases/latest/download/latest.json";
/// Caminho da assinatura.
pub const SIG_PATH: &str = "/dbmodeling/dbml/releases/latest/download/latest.json.sig";
/// Caminho do artefato Linux da versão de teste.
pub const ASSET_PATH: &str =
    "/dbmodeling/dbml/releases/download/v0.4.0/db-modeling-linux-x86_64.tar.gz";
/// URL lógica do artefato (a que vai no manifesto).
pub const ASSET_URL: &str =
    "https://github.com/dbmodeling/dbml/releases/download/v0.4.0/db-modeling-linux-x86_64.tar.gz";
/// Conteúdo do artefato de teste.
pub const ASSET_BODY: &[u8] = b"artefato de teste: conteudo suficiente para truncar no meio";

// --- Servidor ----------------------------------------------------------------------------------

/// Requisição recebida.
#[derive(Debug, Clone)]
pub struct Request {
    pub path: String,
    /// Cabeçalhos com nome em minúsculas.
    pub headers: Vec<(String, String)>,
    /// Quantas requisições a este caminho já chegaram antes desta (0 = primeira).
    pub nth: usize,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        let lowered = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(key, _)| *key == lowered)
            .map(|(_, value)| value.as_str())
    }
}

/// Resposta programada.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Se `Some(n)`, só `n` bytes do corpo são enviados (o `Content-Length` continua cheio) e a
    /// conexão cai: simula download interrompido.
    pub truncate_to: Option<usize>,
}

impl Response {
    pub fn ok(body: &[u8]) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: body.to_vec(),
            truncate_to: None,
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            truncate_to: None,
        }
    }

    pub fn redirect(location: &str) -> Self {
        Self::status(302).with_header("Location", location)
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn truncated(mut self, bytes: usize) -> Self {
        self.truncate_to = Some(bytes);
        self
    }
}

type Handler = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

#[derive(Default)]
struct Shared {
    routes: Mutex<HashMap<String, Handler>>,
    log: Mutex<Vec<Request>>,
}

/// Servidor HTTP local em thread (uma conexão por thread, `Connection: close`).
pub struct TestServer {
    addr: std::net::SocketAddr,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
}

impl TestServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (shared_loop, stop_loop) = (Arc::clone(&shared), Arc::clone(&stop));
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stop_loop.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = stream {
                    let shared = Arc::clone(&shared_loop);
                    thread::spawn(move || handle(stream, &shared));
                }
            }
        });
        Self { addr, shared, stop }
    }

    /// Origem (`http://127.0.0.1:porta`) para `TransportConfig::with_test_origin`.
    pub fn origin(&self) -> Url {
        Url::parse(&format!("http://{}", self.addr)).expect("origem")
    }

    pub fn route(
        &self,
        path: &str,
        handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) {
        self.shared
            .routes
            .lock()
            .expect("routes")
            .insert(path.to_owned(), Arc::new(handler));
    }

    pub fn serve(&self, path: &str, body: &[u8]) {
        let body = body.to_vec();
        self.route(path, move |_| Response::ok(&body));
    }

    pub fn serve_with_etag(&self, path: &str, body: &[u8], etag: &str) {
        let body = body.to_vec();
        let etag = etag.to_owned();
        self.route(path, move |request| {
            if request.header("if-none-match") == Some(etag.as_str()) {
                Response::status(304).with_header("ETag", &etag)
            } else {
                Response::ok(&body).with_header("ETag", &etag)
            }
        });
    }

    pub fn requests(&self) -> Vec<Request> {
        self.shared.log.lock().expect("log").clone()
    }

    pub fn requests_for(&self, path: &str) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|request| request.path == path)
            .collect()
    }

    pub fn total_requests(&self) -> usize {
        self.requests().len()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Acorda o `accept` para a thread sair.
        let _ = TcpStream::connect(self.addr);
    }
}

fn handle(stream: TcpStream, shared: &Shared) {
    let Some(mut request) = read_request(&stream) else {
        return;
    };
    let route_path = request
        .path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned();
    request.path.clone_from(&route_path);
    {
        let mut log = shared.log.lock().expect("log");
        request.nth = log.iter().filter(|r| r.path == route_path).count();
        log.push(request.clone());
    }
    let handler = shared
        .routes
        .lock()
        .expect("routes")
        .get(&route_path)
        .cloned();
    let response = handler.map_or_else(|| Response::status(404), |h| h(&request));
    write_response(stream, &response);
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_owned();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    Some(Request {
        path,
        headers,
        nth: 0,
    })
}

fn write_response(mut stream: TcpStream, response: &Response) {
    let mut head = format!("HTTP/1.1 {} X\r\n", response.status);
    for (name, value) in &response.headers {
        let _ = write!(head, "{name}: {value}\r\n");
    }
    if response.status != 304 {
        let _ = write!(head, "Content-Length: {}\r\n", response.body.len());
    }
    head.push_str("Connection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    let body = match response.truncate_to {
        Some(n) => &response.body[..n.min(response.body.len())],
        None => &response.body[..],
    };
    let _ = stream.write_all(body);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
    // Drena o resto da requisição para o cliente não ver RST.
    let mut sink = [0u8; 64];
    let _ = stream.read(&mut sink);
}

// --- Chaves e assinaturas de TESTE --------------------------------------------------------------

/// Par de chaves minisign **de teste**, descartável (gerado em memória; nunca gravado).
pub struct TestKey {
    pub public_base64: String,
    secret: minisign::SecretKey,
    public: minisign::PublicKey,
}

impl TestKey {
    pub fn generate() -> Self {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("par de teste");
        Self {
            public_base64: pair.pk.to_base64(),
            secret: pair.sk,
            public: pair.pk,
        }
    }

    /// Texto completo (4 linhas) da `.sig`, assinatura prehashed padrão.
    pub fn sign(&self, data: &[u8]) -> String {
        minisign::sign(
            Some(&self.public),
            &self.secret,
            std::io::Cursor::new(data),
            Some("TESTE descartavel"),
            Some("TESTE descartavel"),
        )
        .expect("assinar")
        .into_string()
    }
}

/// Manifesto `schema: 1` só com a plataforma Linux (e as demais com o mesmo artefato).
pub fn manifest_json(version: &str, asset_url: &str, sha256: &str) -> String {
    format!(
        r#"{{"schema":1,"version":"{version}","notes_url":"https://github.com/dbmodeling/dbml/releases/tag/v{version}","platforms":{{"linux-x86_64":{{"url":"{asset_url}","sha256":"{sha256}"}}}}}}"#
    )
}

// --- Ambiente fake -----------------------------------------------------------------------------

/// `PolicyEnv` de teste: Linux, sem arquivo de explicação, variáveis programáveis.
#[derive(Default)]
pub struct FakeEnv {
    pub vars: HashMap<String, String>,
}

impl PolicyEnv for FakeEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
    fn read_file(&self, _path: &Path) -> Option<String> {
        None
    }
    fn target_os(&self) -> Option<TargetOs> {
        Some(TargetOs::Linux)
    }
    fn app_dir(&self) -> Option<PathBuf> {
        None
    }
    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        Some(path.to_path_buf())
    }
    fn is_writable(&self, _dir: &Path) -> bool {
        true
    }
}

// --- Harness -----------------------------------------------------------------------------------

/// Tudo que uma rodada precisa, com o servidor local e uma chave de teste "primária".
pub struct Harness {
    pub server: TestServer,
    pub cache: tempfile::TempDir,
    pub key: TestKey,
    pub reserve_key: TestKey,
    pub state: Mutex<UpdateState>,
    pub etags: MemoryEtagStore,
    pub transport: UreqTransport,
    pub env: FakeEnv,
    pub keys: KeySet,
    pub current: Version,
    pub platform: Option<PlatformKey>,
}

impl Harness {
    pub fn new() -> Self {
        let server = TestServer::start();
        let transport =
            UreqTransport::new(&TransportConfig::default().with_test_origin(server.origin()))
                .expect("transporte");
        let (key, reserve_key) = (TestKey::generate(), TestKey::generate());
        let keys = KeySet::Custom(vec![
            key.public_base64.clone(),
            reserve_key.public_base64.clone(),
        ]);
        Self {
            server,
            cache: tempfile::tempdir().expect("cache"),
            key,
            reserve_key,
            state: Mutex::new(UpdateState::new()),
            etags: MemoryEtagStore::new(),
            transport,
            env: FakeEnv::default(),
            keys,
            current: Version::parse("0.3.4").expect("semver"),
            platform: Some(PlatformKey::LinuxX86_64),
        }
    }

    /// Publica manifesto + assinatura (da chave primária) no servidor.
    pub fn publish(&self, manifest: &str) {
        self.server.serve(MANIFEST_PATH, manifest.as_bytes());
        self.server
            .serve(SIG_PATH, self.key.sign(manifest.as_bytes()).as_bytes());
    }

    /// Publica uma versão `0.4.0` válida do artefato Linux com o conteúdo `ASSET_BODY`.
    pub fn publish_update(&self) {
        self.publish(&manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY)));
        self.server.serve(ASSET_PATH, ASSET_BODY);
    }

    pub fn run(&self, trigger: Trigger, auto_update: bool) -> RoundOutcome {
        let free_space = |_: &Path| -> std::io::Result<u64> { Ok(u64::MAX) };
        let services = Services {
            transport: &self.transport,
            env: &self.env,
            state: &self.state,
            etags: &self.etags,
            free_space: &free_space,
            keys: &self.keys,
        };
        let params = RoundParams {
            cache_dir: self.cache.path(),
            current_version: &self.current,
            platform: self.platform,
            auto_update,
            trigger,
            now: SystemTime::now(),
        };
        run_round(&services, &params)
    }

    pub fn run_auto(&self) -> RoundOutcome {
        self.run(Trigger::Automatic, true)
    }

    pub fn status(&self) -> updater::state::UpdateStatus {
        self.state.lock().expect("state").status().clone()
    }

    pub fn updates_dir(&self) -> PathBuf {
        updater::lock::updates_dir(self.cache.path())
    }
}
