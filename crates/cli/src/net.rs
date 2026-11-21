//! Resolução de `--host` e escolha de porta livre para o servidor local.

use anyhow::Context as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::net::TcpListener;

/// Quantas portas acima da pedida são tentadas quando ela está ocupada.
pub const PORT_RETRIES: u16 = 50;

/// Portas candidatas: `port..=port + PORT_RETRIES`, limitadas a `u16::MAX` (sem estouro).
pub fn candidate_ports(port: u16) -> impl Iterator<Item = u16> {
    port..=port.saturating_add(PORT_RETRIES)
}

/// Resolve `host` (nome como `localhost`, IPv4 ou IPv6 literal, com ou sem colchetes) em
/// endereços de socket para `port`.
///
/// # Errors
///
/// Retorna erro se o nome não resolve para nenhum endereço.
pub async fn resolve_host(host: &str, port: u16) -> anyhow::Result<Vec<SocketAddr>> {
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((bare, port))
        .await
        .with_context(|| format!("could not resolve host `{host}`"))?
        .collect();
    if addrs.is_empty() {
        anyhow::bail!("host `{host}` did not resolve to any address");
    }
    Ok(addrs)
}

/// Escuta em `host`, partindo de `port` e subindo até [`PORT_RETRIES`] portas se estiverem
/// ocupadas. Só `AddrInUse` faz tentar a próxima porta; qualquer outro erro é devolvido.
///
/// # Errors
///
/// Retorna erro se o host não resolve, se não há porta livre na faixa ou se o bind falha por
/// outro motivo.
pub async fn bind_with_retry(host: &str, port: u16) -> anyhow::Result<TcpListener> {
    let mut last_err = None;
    for candidate in candidate_ports(port) {
        let addrs = resolve_host(host, candidate).await?;
        for addr in addrs {
            match TcpListener::bind(addr).await {
                Ok(listener) => return Ok(listener),
                Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => last_err = Some(err),
                Err(err) => {
                    return Err(err).with_context(|| format!("could not listen on {addr}"));
                }
            }
        }
    }
    Err(last_err.map_or_else(
        || anyhow::anyhow!("no port available for `{host}`"),
        anyhow::Error::from,
    ))
    .with_context(|| {
        format!(
            "no free port for `{host}` between {port} and {}",
            port.saturating_add(PORT_RETRIES)
        )
    })
}

/// URL que o usuário abre no navegador: IPv6 entre colchetes e endereço "qualquer" (`0.0.0.0`,
/// `::`) trocado pelo loopback, que é o que o navegador local consegue alcançar.
#[must_use]
pub fn browser_url(addr: SocketAddr, token: &str) -> String {
    let ip = match addr.ip() {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}:{}/?token={token}", addr.port()),
        IpAddr::V6(v6) => format!("http://[{v6}]:{}/?token={token}", addr.port()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_ports_do_not_overflow_u16() {
        assert_eq!(candidate_ports(3000).count(), 51);
        assert_eq!(candidate_ports(65530).last(), Some(65535));
        assert_eq!(candidate_ports(65530).count(), 6);
        assert_eq!(candidate_ports(65535).collect::<Vec<_>>(), vec![65535]);
    }

    #[tokio::test]
    async fn resolves_names_and_bare_ipv6() {
        let local = resolve_host("localhost", 3000).await.unwrap();
        assert!(
            local
                .iter()
                .all(|a| a.ip().is_loopback() && a.port() == 3000)
        );
        let v6 = resolve_host("::1", 3000).await.unwrap();
        assert_eq!(v6, vec!["[::1]:3000".parse::<SocketAddr>().unwrap()]);
        let bracketed = resolve_host("[::1]", 3000).await.unwrap();
        assert_eq!(bracketed, v6);
        let v4 = resolve_host("127.0.0.1", 80).await.unwrap();
        assert_eq!(v4, vec!["127.0.0.1:80".parse::<SocketAddr>().unwrap()]);
        assert!(resolve_host("host.invalido.", 80).await.is_err());
    }

    #[tokio::test]
    async fn binds_localhost_and_skips_busy_ports() {
        let first = bind_with_retry("localhost", 0).await.unwrap();
        let busy = first.local_addr().unwrap().port();
        let second = bind_with_retry("127.0.0.1", busy).await;
        // A porta de `first` está ocupada no IPv4 se `first` ficou em 127.0.0.1; nos dois casos
        // o resultado precisa ser um listener válido numa porta >= `busy`.
        let second = second.unwrap();
        assert!(second.local_addr().unwrap().port() >= busy);
    }

    #[tokio::test]
    async fn high_port_does_not_panic() {
        // Na faixa final nada deve estourar, mesmo que nenhuma porta esteja livre.
        let _ = bind_with_retry("127.0.0.1", 65535).await;
        let _ = bind_with_retry("127.0.0.1", 65530).await;
    }

    #[test]
    fn browser_url_brackets_ipv6_and_avoids_unspecified() {
        let a: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        assert_eq!(browser_url(a, "t"), "http://127.0.0.1:3000/?token=t");
        let a: SocketAddr = "[::1]:3000".parse().unwrap();
        assert_eq!(browser_url(a, "t"), "http://[::1]:3000/?token=t");
        let a: SocketAddr = "0.0.0.0:3000".parse().unwrap();
        assert_eq!(browser_url(a, "t"), "http://127.0.0.1:3000/?token=t");
        let a: SocketAddr = "[::]:3000".parse().unwrap();
        assert_eq!(browser_url(a, "t"), "http://[::1]:3000/?token=t");
    }
}
