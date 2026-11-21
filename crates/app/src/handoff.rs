//! Entrega de arquivos para o app já aberto: instância única por usuário (#69) e abertura por
//! evento do sistema, como o "Abrir com" do Finder (#70).
//!
//! As duas fontes caem na mesma caixa de entrada (`push`); o shell a consulta a cada 250 ms
//! (`take`) e abre os arquivos/pastas na janela existente.
//!
//! Instância única: a primeira instância escuta em `127.0.0.1:<porta efêmera>` e grava
//! `instance.lock` (porta + token, só o dono lê) na pasta de configuração. A segunda instância
//! lê o lock, envia seus argumentos com o token e sai. Lock velho (porta fechada) é ignorado.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Pedido recebido de outra instância ou do sistema.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// arquivos/pastas a abrir (vazio = só trazer a janela para a frente)
    pub paths: Vec<PathBuf>,
}

static INBOX: Mutex<Vec<Request>> = Mutex::new(Vec::new());

pub fn push(request: Request) {
    if let Ok(mut inbox) = INBOX.lock() {
        inbox.push(request);
    }
}

pub fn take() -> Vec<Request> {
    INBOX.lock().map(|mut inbox| std::mem::take(&mut *inbox)).unwrap_or_default()
}

/// Converte URLs entregues pelo sistema (`file:///Users/a/b%20c.dbml`) em caminhos. Caminhos
/// puros passam; outros esquemas (`http:`...) são ignorados.
pub fn paths_from_urls(urls: &[String]) -> Vec<PathBuf> {
    urls.iter().filter_map(|u| path_from_url(u)).collect()
}

fn path_from_url(url: &str) -> Option<PathBuf> {
    let rest = if let Some(r) = url.strip_prefix("file://") {
        r.strip_prefix("localhost").unwrap_or(r)
    } else if url.contains("://") || url.is_empty() {
        return None;
    } else {
        return Some(PathBuf::from(url));
    };
    let decoded = percent_decode(rest)?;
    // `file:///C:/x` → `C:/x` (Windows)
    let bytes = decoded.as_bytes();
    let path = if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        decoded[1..].to_owned()
    } else {
        decoded
    };
    Some(PathBuf::from(path))
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("instance.lock")
}

fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    h.write_u64(crate::session::now_secs());
    let a = h.finish();
    let b = std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

/// Envia `paths` para a instância que escreveu o lock em `dir`. `true` = entregue (o chamador
/// deve sair); `false` = não há instância viva (lock ausente, velho ou recusado).
pub fn try_forward(dir: &Path, paths: &[PathBuf]) -> bool {
    let Ok(lock) = std::fs::read_to_string(lock_path(dir)) else { return false };
    let mut parts = lock.split_whitespace();
    let (Some(port), Some(token)) = (parts.next().and_then(|p| p.parse::<u16>().ok()), parts.next()) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return false };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut message = format!("{token}\n");
    for p in paths {
        message.push_str(&p.to_string_lossy());
        message.push('\n');
    }
    if stream.write_all(message.as_bytes()).is_err() || stream.shutdown(std::net::Shutdown::Write).is_err() {
        return false;
    }
    // a instância responde "ok" só depois de validar o token
    let mut ack = String::new();
    stream.read_to_string(&mut ack).is_ok() && ack.trim() == "ok"
}

/// Torna este processo a instância principal: escuta e entrega cada pedido a `on_request`.
pub fn serve(dir: &Path, on_request: impl Fn(Request) + Send + 'static) -> std::io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let token = new_token();
    std::fs::create_dir_all(dir)?;
    let lock = lock_path(dir);
    crate::session::atomic_write_sync(&lock, &format!("{port} {token}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600));
    }
    std::thread::Builder::new().name("instance-listener".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            handle(stream, &token, &on_request);
        }
    })?;
    Ok(())
}

fn handle(mut stream: TcpStream, token: &str, on_request: &impl Fn(Request)) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut body = String::new();
    // limite de 1 MiB: um cliente malicioso não enche a memória
    if (&stream).take(1 << 20).read_to_string(&mut body).is_err() {
        return;
    }
    let mut lines = body.lines();
    if lines.next() != Some(token) {
        return;
    }
    let paths = lines.filter(|l| !l.is_empty()).map(PathBuf::from).collect();
    on_request(Request { paths });
    let _ = stream.write_all(b"ok\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm-handoff-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn urls_become_paths() {
        let urls = [
            "file:///Users/ana/My%20Models/a.dbml".to_string(),
            "file://localhost/tmp/b.dbml".to_string(),
            "file:///C:/dados/c.dbml".to_string(),
            "/home/x/d.dbml".to_string(),
            "https://example.com/e.dbml".to_string(),
        ];
        assert_eq!(
            paths_from_urls(&urls),
            [
                PathBuf::from("/Users/ana/My Models/a.dbml"),
                PathBuf::from("/tmp/b.dbml"),
                PathBuf::from("C:/dados/c.dbml"),
                PathBuf::from("/home/x/d.dbml"),
            ]
        );
        assert_eq!(path_from_url("file:///bad%zz"), None);
    }

    #[test]
    fn second_instance_hands_off_arguments() {
        let dir = temp_dir("ok");
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        serve(&dir, move |r| sink.lock().unwrap().push(r)).unwrap();
        let sent = [PathBuf::from("/tmp/a.dbml"), PathBuf::from("/tmp/b b.dbml")];
        assert!(try_forward(&dir, &sent));
        assert!(try_forward(&dir, &[]));
        let got = got.lock().unwrap();
        assert_eq!(got[0].paths, sent);
        assert_eq!(got[1], Request::default());
        drop(got);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_or_missing_lock_means_no_running_instance() {
        let dir = temp_dir("stale");
        assert!(!try_forward(&dir, &[]));
        // porta que ninguém escuta
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        std::fs::write(lock_path(&dir), format!("{port} tok\n")).unwrap();
        assert!(!try_forward(&dir, &[]));
        std::fs::write(lock_path(&dir), "lixo").unwrap();
        assert!(!try_forward(&dir, &[]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_token_is_rejected() {
        let dir = temp_dir("token");
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        serve(&dir, move |r| sink.lock().unwrap().push(r)).unwrap();
        let lock = std::fs::read_to_string(lock_path(&dir)).unwrap();
        let port: u16 = lock.split_whitespace().next().unwrap().parse().unwrap();
        std::fs::write(lock_path(&dir), format!("{port} errado\n")).unwrap();
        assert!(!try_forward(&dir, &[PathBuf::from("/tmp/x.dbml")]));
        assert!(got.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
