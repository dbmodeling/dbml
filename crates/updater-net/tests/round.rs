//! Rodada completa contra um servidor HTTP local (sem rede externa), com o gancho de teste.
//! Chaves e assinaturas são de TESTE, geradas em memória.

#![cfg(any(debug_assertions, feature = "harness"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::time::{Duration, SystemTime};

use updater::lock::UpdateLock;
use updater::state::{Trigger, UpdateStatus};
use updater::transport::{Transport, TransportError};
use updater::verify::{VerifyError, sha256_hex};
use updater_net::constants::{MAX_MANIFEST_BYTES, MAX_REDIRECTS, MAX_SIGNATURE_BYTES};
use updater_net::etag::{EtagRecord, EtagStore};
use updater_net::fetch::FetchError;
use updater_net::keys::KeySet;
use updater_net::round::{RoundError, RoundOutcome, SkipReason};

use support::{
    ASSET_BODY, ASSET_PATH, ASSET_URL, Harness, MANIFEST_PATH, Response, SIG_PATH, manifest_json,
};

const CHECKSUM_OTHER: &[u8] = b"outro conteudo";

fn failed(outcome: RoundOutcome) -> RoundError {
    match outcome {
        RoundOutcome::Failed(error) => error,
        other => panic!("esperava Failed, veio {other:?}"),
    }
}

fn ready(outcome: RoundOutcome) -> updater_net::round::VerifiedUpdate {
    match outcome {
        RoundOutcome::Ready(update) => update,
        other => panic!("esperava Ready, veio {other:?}"),
    }
}

fn skipped(outcome: RoundOutcome) -> SkipReason {
    match outcome {
        RoundOutcome::Skipped(reason) => reason,
        other => panic!("esperava Skipped, veio {other:?}"),
    }
}

fn network_message(error: &RoundError) -> String {
    match error {
        RoundError::Fetch(FetchError::Transport(TransportError::Network(message)))
        | RoundError::Transport(TransportError::Network(message)) => message.clone(),
        other => panic!("esperava erro de rede, veio {other:?}"),
    }
}

// --- Caminho feliz -----------------------------------------------------------------------------

#[test]
fn caminho_feliz_entrega_arquivo_verificado_e_deixa_o_estado_em_installing() {
    let h = Harness::new();
    h.publish_update();

    let update = ready(h.run_auto());

    assert_eq!(update.version().to_string(), "0.4.0");
    assert_eq!(update.sha256(), sha256_hex(ASSET_BODY));
    assert_eq!(fs::read(update.path()).expect("ler"), ASSET_BODY);
    assert!(update.path().starts_with(h.updates_dir().join("0.4.0")));
    assert_eq!(
        update.notes_url(),
        "https://github.com/dbmodeling/dbml/releases/tag/v0.4.0"
    );
    assert_eq!(update.team_id(), None);
    update.recheck().expect("hash ainda confere");
    assert!(matches!(h.status(), UpdateStatus::Installing(v) if v.to_string() == "0.4.0"));
    // `found_update` limpa o ETag: nenhuma falha futura vira "sem novidade".
    assert_eq!(h.etags.load(), None);
    // Sem .part sobrando.
    let leftovers: Vec<_> = fs::read_dir(h.updates_dir().join("0.4.0"))
        .expect("ls")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
}

#[test]
fn so_versao_so_e_user_agent_saem_ao_servidor() {
    let h = Harness::new();
    h.publish_update();
    drop(ready(h.run_auto()));

    let expected_agent = format!("db-modeling-updater/{}", env!("CARGO_PKG_VERSION"));
    let allowed = [
        "host",
        "user-agent",
        "accept",
        "accept-encoding",
        "if-none-match",
        "range",
    ];
    for request in h.server.requests() {
        assert_eq!(request.header("user-agent"), Some(expected_agent.as_str()));
        for (name, _) in &request.headers {
            assert!(
                allowed.contains(&name.as_str()),
                "cabeçalho inesperado enviado ao servidor: {name}"
            );
        }
    }
}

#[test]
fn rodada_manual_tambem_funciona_com_auto_update_desligado() {
    let h = Harness::new();
    h.publish_update();
    drop(ready(h.run(Trigger::Manual, false)));
}

// --- Assinatura --------------------------------------------------------------------------------

#[test]
fn manifesto_adulterado_e_recusado_sem_baixar_o_artefato() {
    let h = Harness::new();
    let signed = manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY));
    let tampered = manifest_json("0.4.0", ASSET_URL, &sha256_hex(CHECKSUM_OTHER));
    h.server.serve(MANIFEST_PATH, tampered.as_bytes());
    h.server
        .serve(SIG_PATH, h.key.sign(signed.as_bytes()).as_bytes());
    h.server.serve(ASSET_PATH, ASSET_BODY);

    let error = failed(h.run_auto());

    assert!(
        matches!(error, RoundError::Signature(VerifyError::BadSignature)),
        "{error:?}"
    );
    assert!(h.server.requests_for(ASSET_PATH).is_empty());
    assert_eq!(
        h.status(),
        UpdateStatus::Idle,
        "automática falha em silêncio"
    );
    assert_eq!(h.etags.load(), None);
}

#[test]
fn assinatura_de_chave_desconhecida_e_recusada() {
    let h = Harness::new();
    let manifest = manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server.serve(MANIFEST_PATH, manifest.as_bytes());
    let stranger = support::TestKey::generate();
    h.server
        .serve(SIG_PATH, stranger.sign(manifest.as_bytes()).as_bytes());

    let error = failed(h.run_auto());

    assert!(matches!(
        error,
        RoundError::Signature(VerifyError::BadSignature)
    ));
    assert!(h.server.requests_for(ASSET_PATH).is_empty());
}

#[test]
fn assinatura_malformada_e_recusada() {
    let h = Harness::new();
    h.publish_update();
    h.server.serve(SIG_PATH, b"isto nao e uma assinatura");
    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Signature(VerifyError::MalformedSignature)
    ));
}

#[test]
fn assinatura_so_pela_chave_reserva_e_aceita_e_sem_ela_nao() {
    let h = Harness::new();
    let manifest = manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server.serve(MANIFEST_PATH, manifest.as_bytes());
    h.server
        .serve(SIG_PATH, h.reserve_key.sign(manifest.as_bytes()).as_bytes());
    h.server.serve(ASSET_PATH, ASSET_BODY);
    drop(ready(h.run_auto()));

    // Mesma publicação, mas só com a primária na lista: a assinatura da reserva não vale.
    let mut only_primary = Harness::new();
    only_primary.keys = KeySet::Custom(vec![only_primary.key.public_base64.clone()]);
    only_primary
        .server
        .serve(MANIFEST_PATH, manifest.as_bytes());
    only_primary.server.serve(
        SIG_PATH,
        only_primary
            .reserve_key
            .sign(manifest.as_bytes())
            .as_bytes(),
    );
    only_primary.server.serve(ASSET_PATH, ASSET_BODY);
    let error = failed(only_primary.run_auto());
    assert!(matches!(
        error,
        RoundError::Signature(VerifyError::BadSignature)
    ));
}

#[test]
fn chaves_embutidas_de_producao_recusam_assinatura_de_teste() {
    let mut h = Harness::new();
    h.keys = KeySet::Production;
    h.publish_update();
    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Signature(VerifyError::BadSignature)
    ));
}

#[test]
fn manifesto_e_sig_de_releases_diferentes_e_falha_transitoria() {
    // Uma release é publicada entre os dois GETs: o manifesto é da nova, a .sig da antiga.
    let h = Harness::new();
    let old = manifest_json("0.3.9", ASSET_URL, &sha256_hex(CHECKSUM_OTHER));
    let new = manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server
        .serve_with_etag(MANIFEST_PATH, new.as_bytes(), "\"v-new\"");
    h.server
        .serve(SIG_PATH, h.key.sign(old.as_bytes()).as_bytes());

    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Signature(VerifyError::BadSignature)
    ));
    assert_eq!(h.status(), UpdateStatus::Idle, "silencioso no automático");
    assert_eq!(h.etags.load(), None, "ETag não é gravado");

    let manual = failed(h.run(Trigger::Manual, true));
    assert!(matches!(manual, RoundError::Signature(_)));
    assert_eq!(h.status(), UpdateStatus::Errored, "Errored no manual");

    // Quando as duas se alinham, a rodada seguinte funciona.
    h.server
        .serve(SIG_PATH, h.key.sign(new.as_bytes()).as_bytes());
    h.server.serve(ASSET_PATH, ASSET_BODY);
    drop(ready(h.run_auto()));
}

// --- Hash e artefato ---------------------------------------------------------------------------

#[test]
fn hash_divergente_ao_baixar_e_recusado_e_nada_sobra() {
    let h = Harness::new();
    h.publish(&manifest_json(
        "0.4.0",
        ASSET_URL,
        &sha256_hex(CHECKSUM_OTHER),
    ));
    h.server.serve(ASSET_PATH, ASSET_BODY);

    let error = failed(h.run_auto());

    assert!(
        matches!(error, RoundError::Fetch(FetchError::HashMismatch(_))),
        "{error:?}"
    );
    assert_eq!(
        fs::read_dir(h.updates_dir().join("0.4.0"))
            .expect("ls")
            .count(),
        0
    );
    assert_eq!(h.status(), UpdateStatus::Idle);
}

#[test]
fn recalculo_do_hash_detecta_arquivo_trocado_depois_da_verificacao() {
    let h = Harness::new();
    h.publish_update();
    let update = ready(h.run_auto());
    update.recheck().expect("ok antes");
    fs::write(update.path(), b"trocado por outro processo").expect("adulterar");
    assert!(matches!(
        update.recheck(),
        Err(FetchError::RecheckMismatch(_))
    ));
}

#[test]
fn url_do_artefato_fora_do_repositorio_e_recusada_sem_requisicao() {
    for outside in [
        "https://github.com/outro/repo/releases/download/v0.4.0/app.tar.gz",
        "https://evil.example/dbmodeling/dbml/releases/download/v0.4.0/app.tar.gz",
        "http://github.com/dbmodeling/dbml/releases/download/v0.4.0/app.tar.gz",
        "https://github.com/dbmodeling/dbml/releases/download/%2e%2e/%2e%2e/x/app.tar.gz",
    ] {
        let h = Harness::new();
        h.publish(&manifest_json("0.4.0", outside, &sha256_hex(ASSET_BODY)));
        h.server.serve(ASSET_PATH, ASSET_BODY);

        let error = failed(h.run_auto());

        assert!(
            matches!(error, RoundError::Manifest(_)),
            "{outside}: {error:?}"
        );
        assert!(h.server.requests_for(ASSET_PATH).is_empty(), "{outside}");
    }
}

#[test]
fn versao_igual_menor_ou_prerelease_nao_baixa_nada() {
    for version in ["0.3.4", "0.3.3", "0.1.0", "0.5.0-rc.1"] {
        let h = Harness::new();
        h.publish(&manifest_json(version, ASSET_URL, &sha256_hex(ASSET_BODY)));
        h.server.serve(ASSET_PATH, ASSET_BODY);

        assert!(matches!(h.run_auto(), RoundOutcome::UpToDate), "{version}");
        assert!(h.server.requests_for(ASSET_PATH).is_empty(), "{version}");
        assert_eq!(h.status(), UpdateStatus::Idle);
    }
}

#[test]
fn rodada_manual_sem_versao_nova_mostra_atualizado() {
    let h = Harness::new();
    h.publish(&manifest_json("0.3.4", ASSET_URL, &sha256_hex(ASSET_BODY)));
    assert!(matches!(
        h.run(Trigger::Manual, true),
        RoundOutcome::UpToDate
    ));
    assert_eq!(h.status(), UpdateStatus::UpToDate);
}

#[test]
fn esquema_desconhecido_e_ignorado_como_sem_atualizacao() {
    let h = Harness::new();
    h.publish(r#"{"schema":2,"version":"9.9.9"}"#);
    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));
}

#[test]
fn versao_ja_instalada_aguardando_reinicio_nao_baixa_de_novo() {
    let h = Harness::new();
    h.publish_update();
    let update = ready(h.run_auto());
    // O instalador conclui: `Updated(0.4.0)`.
    h.state
        .lock()
        .expect("state")
        .installed()
        .expect("installed");
    drop(update);

    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));
    assert_eq!(h.server.requests_for(ASSET_PATH).len(), 1);
}

#[test]
fn plataforma_ausente_no_manifesto_e_falha() {
    let h = Harness::new();
    h.publish(&manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY)));
    let manifest_without_linux = r#"{"schema":1,"version":"0.4.0","notes_url":"https://github.com/dbmodeling/dbml/releases/tag/v0.4.0","platforms":{}}"#;
    h.publish(manifest_without_linux);
    let error = failed(h.run_auto());
    assert!(matches!(error, RoundError::Manifest(_)));
}

// --- Redirecionamento --------------------------------------------------------------------------

#[test]
fn redirecionamento_para_host_nao_permitido_e_recusado() {
    for target in [
        "https://evil.example/blob/x",
        "https://github.com.evil.example/blob/x",
        "https://evilgithubusercontent.com/blob/x",
        "http://github.com/dbmodeling/dbml/releases/download/v0.4.0/x",
        "https://user@github.com/dbmodeling/dbml/releases/x",
        "https://github.com:8443/dbmodeling/dbml/releases/x",
        "//evil.example/blob/x",
    ] {
        let h = Harness::new();
        h.publish_update();
        let location = target.to_owned();
        h.server
            .route(ASSET_PATH, move |_| Response::redirect(&location));

        let error = failed(h.run_auto());

        let message = network_message(&error);
        assert!(message.contains("recusada"), "{target}: {message}");
        assert_eq!(h.server.requests_for(ASSET_PATH).len(), 1, "{target}");
        assert_eq!(
            h.server.total_requests(),
            3,
            "{target}: nada além dos 3 GETs"
        );
        assert!(
            !h.updates_dir()
                .join("0.4.0")
                .join("db-modeling-linux-x86_64.tar.gz")
                .exists()
        );
    }
}

#[test]
fn redirecionamento_com_travessia_codificada_e_recusado() {
    for target in [
        "/dbmodeling/dbml/releases/%2e%2e/%2e%2e/evil/x",
        "/dbmodeling/dbml/releases/%2E%2E/evil/x",
        "https://github.com/dbmodeling/dbml/releases/download/../../../x",
        "https://objects.githubusercontent.com/a/%2e%2e/b",
        "https://objects.githubusercontent.com/a%2fb",
    ] {
        let h = Harness::new();
        h.publish_update();
        let location = target.to_owned();
        h.server
            .route(ASSET_PATH, move |_| Response::redirect(&location));

        let error = failed(h.run_auto());

        let message = network_message(&error);
        assert!(message.contains("recusada"), "{target}: {message}");
        assert_eq!(h.server.total_requests(), 3, "{target}");
    }
}

#[test]
fn redirecionamento_para_cdn_do_github_e_seguido() {
    let h = Harness::new();
    h.publish_update();
    // github.com → github.com (releases/download) → CDN, como o `latest/download` real.
    h.server.route(ASSET_PATH, |_| {
        Response::redirect("/dbmodeling/dbml/releases/download/v0.4.0/hop.bin")
    });
    h.server
        .route("/dbmodeling/dbml/releases/download/v0.4.0/hop.bin", |_| {
            Response::redirect(
                "https://release-assets.githubusercontent.com/blob/abc?sig=a%2Bb&x=%2e%2e",
            )
        });
    h.server.serve("/blob/abc", ASSET_BODY);

    let update = ready(h.run_auto());

    assert_eq!(fs::read(update.path()).expect("ler"), ASSET_BODY);
    assert_eq!(h.server.requests_for("/blob/abc").len(), 1);
}

#[test]
fn laco_de_redirecionamento_e_interrompido_no_limite() {
    let h = Harness::new();
    h.publish_update();
    h.server
        .route(ASSET_PATH, |_| Response::redirect(ASSET_URL));

    let error = failed(h.run_auto());

    assert!(network_message(&error).contains("demais"));
    assert_eq!(
        h.server.requests_for(ASSET_PATH).len(),
        MAX_REDIRECTS as usize + 1
    );
}

#[test]
fn manifesto_redirecionado_a_host_estranho_nao_chega_a_pedir_a_assinatura() {
    let h = Harness::new();
    h.server.route(MANIFEST_PATH, |_| {
        Response::redirect("https://evil.example/latest.json")
    });
    let error = failed(h.run_auto());
    assert!(network_message(&error).contains("recusada"));
    assert!(h.server.requests_for(SIG_PATH).is_empty());
}

// --- Arquivo parcial ---------------------------------------------------------------------------

fn serve_with_range(h: &Harness, truncate_first: Option<usize>) {
    h.server.route(ASSET_PATH, move |request| {
        if let Some(range) = request.header("range") {
            let start: usize = range
                .trim_start_matches("bytes=")
                .trim_end_matches('-')
                .parse()
                .expect("Range simples");
            return Response {
                status: 206,
                headers: vec![(
                    "Content-Range".to_owned(),
                    format!(
                        "bytes {start}-{}/{}",
                        ASSET_BODY.len() - 1,
                        ASSET_BODY.len()
                    ),
                )],
                body: ASSET_BODY[start..].to_vec(),
                truncate_to: None,
            };
        }
        let response = Response::ok(ASSET_BODY);
        match truncate_first {
            Some(bytes) if request.nth == 0 => response.truncated(bytes),
            _ => response,
        }
    });
}

#[test]
fn download_interrompido_deixa_so_o_part_e_a_proxima_rodada_retoma_por_range() {
    let h = Harness::new();
    h.publish_update();
    serve_with_range(&h, Some(10));

    let error = failed(h.run_auto());

    assert!(matches!(
        error,
        RoundError::Fetch(FetchError::Transport(TransportError::Network(_)))
    ));
    let dir = h.updates_dir().join("0.4.0");
    let part = dir.join("db-modeling-linux-x86_64.tar.gz.part");
    assert_eq!(fs::read(&part).expect("part").len(), 10, "só o parcial");
    assert!(
        !dir.join("db-modeling-linux-x86_64.tar.gz").exists(),
        "o parcial nunca vira o arquivo final"
    );

    let update = ready(h.run_auto());

    assert_eq!(fs::read(update.path()).expect("ler"), ASSET_BODY);
    let asset_requests = h.server.requests_for(ASSET_PATH);
    assert_eq!(asset_requests.len(), 2);
    assert_eq!(asset_requests[1].header("range"), Some("bytes=10-"));
    assert!(!part.exists());
}

#[test]
fn part_corrompido_retomado_nao_e_usado_e_o_hash_o_descarta() {
    let h = Harness::new();
    h.publish_update();
    serve_with_range(&h, None);
    let dir = h.updates_dir().join("0.4.0");
    fs::create_dir_all(&dir).expect("dir");
    let part = dir.join("db-modeling-linux-x86_64.tar.gz.part");
    fs::write(&part, b"LIXO").expect("part corrompido");

    let error = failed(h.run_auto());

    assert!(matches!(
        error,
        RoundError::Fetch(FetchError::HashMismatch(_))
    ));
    assert!(!part.exists(), "parcial inválido é apagado");
    assert!(!dir.join("db-modeling-linux-x86_64.tar.gz").exists());

    // A rodada seguinte recomeça do zero e conclui.
    drop(ready(h.run_auto()));
}

#[test]
fn servidor_que_ignora_range_reinicia_o_download_do_zero() {
    let h = Harness::new();
    h.publish_update();
    let dir = h.updates_dir().join("0.4.0");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("db-modeling-linux-x86_64.tar.gz.part"),
        &ASSET_BODY[..7],
    )
    .expect("part");
    // Responde 200 com o corpo inteiro mesmo com `Range`.
    h.server.serve(ASSET_PATH, ASSET_BODY);

    let update = ready(h.run_auto());

    assert_eq!(fs::read(update.path()).expect("ler"), ASSET_BODY);
}

// --- ETag --------------------------------------------------------------------------------------

#[test]
fn segunda_rodada_usa_if_none_match_e_o_304_encerra_sem_pedir_a_assinatura() {
    let h = Harness::new();
    let manifest = manifest_json("0.3.4", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server
        .serve_with_etag(MANIFEST_PATH, manifest.as_bytes(), "\"v1\"");
    h.server
        .serve(SIG_PATH, h.key.sign(manifest.as_bytes()).as_bytes());

    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));
    assert_eq!(h.etags.load().map(|r| r.etag), Some("\"v1\"".to_owned()));

    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));

    let manifest_requests = h.server.requests_for(MANIFEST_PATH);
    assert_eq!(manifest_requests.len(), 2);
    assert_eq!(manifest_requests[0].header("if-none-match"), None);
    assert_eq!(manifest_requests[1].header("if-none-match"), Some("\"v1\""));
    assert_eq!(
        h.server.requests_for(SIG_PATH).len(),
        1,
        "304 não pede a .sig"
    );
}

#[test]
fn etag_atravessa_o_redirecionamento_do_manifesto() {
    let h = Harness::new();
    let manifest = manifest_json("0.3.4", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server.route(MANIFEST_PATH, |_| {
        Response::redirect("https://objects.githubusercontent.com/blob/m")
    });
    h.server
        .serve_with_etag("/blob/m", manifest.as_bytes(), "\"cdn-1\"");
    h.server
        .serve(SIG_PATH, h.key.sign(manifest.as_bytes()).as_bytes());

    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));
    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));

    let cdn = h.server.requests_for("/blob/m");
    assert_eq!(cdn.len(), 2);
    assert_eq!(cdn[1].header("if-none-match"), Some("\"cdn-1\""));
}

#[test]
fn download_que_falha_nao_grava_o_etag_e_a_proxima_rodada_nao_manda_if_none_match() {
    let h = Harness::new();
    let manifest = manifest_json("0.4.0", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server
        .serve_with_etag(MANIFEST_PATH, manifest.as_bytes(), "\"v2\"");
    h.server
        .serve(SIG_PATH, h.key.sign(manifest.as_bytes()).as_bytes());
    h.server.route(ASSET_PATH, |_| Response::status(500));
    h.etags.save(EtagRecord {
        etag: "\"v1\"".to_owned(),
        current: h.current.clone(),
        pending: None,
    });

    let error = failed(h.run_auto());

    assert!(matches!(
        error,
        RoundError::Fetch(FetchError::Transport(TransportError::Status(500)))
    ));
    assert_eq!(
        h.etags.load(),
        None,
        "nada gravado; o antigo também foi limpo"
    );
    assert_eq!(
        h.server.requests_for(MANIFEST_PATH)[0].header("if-none-match"),
        Some("\"v1\""),
        "a primeira rodada ainda usou o ETag antigo"
    );

    // A atualização continua disponível na rodada seguinte (não virou 304 "sem novidade").
    h.server.serve(ASSET_PATH, ASSET_BODY);
    drop(ready(h.run_auto()));
    assert_eq!(
        h.server.requests_for(MANIFEST_PATH)[1].header("if-none-match"),
        None
    );
}

#[test]
fn etag_de_outra_versao_em_execucao_e_ignorado() {
    let h = Harness::new();
    let manifest = manifest_json("0.3.4", ASSET_URL, &sha256_hex(ASSET_BODY));
    h.server
        .serve_with_etag(MANIFEST_PATH, manifest.as_bytes(), "\"v1\"");
    h.server
        .serve(SIG_PATH, h.key.sign(manifest.as_bytes()).as_bytes());
    h.etags.save(EtagRecord {
        etag: "\"v1\"".to_owned(),
        current: semver::Version::parse("0.3.3").expect("semver"),
        pending: None,
    });

    assert!(matches!(h.run_auto(), RoundOutcome::UpToDate));

    assert_eq!(
        h.server.requests_for(MANIFEST_PATH)[0].header("if-none-match"),
        None
    );
}

// --- Política: nenhuma requisição --------------------------------------------------------------

#[test]
fn auto_update_desligado_nao_faz_requisicao_automatica() {
    let h = Harness::new();
    h.publish_update();

    let reason = skipped(h.run(Trigger::Automatic, false));

    assert_eq!(reason, SkipReason::AutoUpdateOff);
    assert_eq!(h.server.total_requests(), 0);
    assert_eq!(h.status(), UpdateStatus::Idle);
}

#[test]
fn razao_de_desativacao_bloqueia_automatica_e_manual_sem_requisicao() {
    for trigger in [Trigger::Automatic, Trigger::Manual] {
        let mut h = Harness::new();
        h.env.vars.insert(
            updater::constants::ENV_UPDATE_EXPLANATION.to_owned(),
            "instalado pelo gerenciador de pacotes".to_owned(),
        );
        h.publish_update();

        let reason = skipped(h.run(trigger, true));

        assert!(matches!(reason, SkipReason::Disabled(_)), "{trigger:?}");
        assert_eq!(h.server.total_requests(), 0, "{trigger:?}");
        assert_eq!(h.status(), UpdateStatus::Idle);
        h.platform = None;
    }
}

#[test]
fn plataforma_sem_artefato_nao_faz_requisicao() {
    let mut h = Harness::new();
    h.platform = None;
    h.publish_update();
    assert_eq!(skipped(h.run_auto()), SkipReason::UnsupportedPlatform);
    assert_eq!(h.server.total_requests(), 0);
}

// --- Trava -------------------------------------------------------------------------------------

#[test]
fn segunda_instancia_sem_a_trava_pula_a_rodada_e_nao_limpa_nada() {
    let h = Harness::new();
    h.publish_update();
    let held = UpdateLock::try_acquire(h.cache.path())
        .expect("io")
        .expect("trava");
    let orphan = h.updates_dir().join("orfao-antigo.bin");
    fs::write(&orphan, b"x").expect("orfão");
    let old = SystemTime::now() - Duration::from_hours(48);
    fs::OpenOptions::new()
        .write(true)
        .open(&orphan)
        .expect("abrir")
        .set_modified(old)
        .expect("mtime");

    let reason = skipped(h.run_auto());

    assert_eq!(reason, SkipReason::LockBusy);
    assert_eq!(h.server.total_requests(), 0);
    assert_eq!(h.status(), UpdateStatus::Idle);
    assert!(orphan.exists(), "sem a trava, nada é varrido");

    // Com a trava liberada, a rodada varre o órfão (trava ANTES do sweep) e segue.
    drop(held);
    drop(ready(h.run_auto()));
    assert!(!orphan.exists(), "órfão com mais de 24 h foi varrido");
}

#[test]
fn atualizacao_pronta_segura_a_trava_ate_ser_descartada() {
    let h = Harness::new();
    h.publish_update();
    let update = ready(h.run_auto());

    assert!(
        UpdateLock::try_acquire(h.cache.path())
            .expect("io")
            .is_none(),
        "o instalador ainda detém a trava"
    );
    assert_eq!(skipped(h.run_auto()), SkipReason::LockBusy);

    drop(update);
    assert!(
        UpdateLock::try_acquire(h.cache.path())
            .expect("io")
            .is_some()
    );
}

// --- Limites -----------------------------------------------------------------------------------

#[test]
fn manifesto_e_assinatura_acima_do_limite_sao_recusados() {
    let h = Harness::new();
    h.publish_update();
    let huge = vec![b'x'; usize::try_from(MAX_MANIFEST_BYTES).expect("cabe") + 1];
    h.server.serve(MANIFEST_PATH, &huge);
    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Transport(TransportError::TooLarge(MAX_MANIFEST_BYTES))
    ));

    let h = Harness::new();
    h.publish_update();
    let huge = vec![b'x'; usize::try_from(MAX_SIGNATURE_BYTES).expect("cabe") + 1];
    h.server.serve(SIG_PATH, &huge);
    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Transport(TransportError::TooLarge(MAX_SIGNATURE_BYTES))
    ));
}

#[test]
fn download_acima_do_limite_e_recusado_e_nao_deixa_arquivo() {
    let h = Harness::new();
    h.server.serve(ASSET_PATH, ASSET_BODY);
    let dest = h.cache.path().join("grande.part");

    let result = h.transport.download(ASSET_URL, &dest, 10);

    assert_eq!(result, Err(TransportError::TooLarge(10)));
    assert!(!dest.exists());
}

#[test]
fn status_de_erro_do_servidor_e_propagado() {
    let h = Harness::new();
    h.server.route(MANIFEST_PATH, |_| Response::status(503));
    let error = failed(h.run_auto());
    assert!(matches!(
        error,
        RoundError::Transport(TransportError::Status(503))
    ));
}

// --- Diretório privado -------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn downloads_ficam_em_diretorio_0700() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new();
    h.publish_update();
    let update = ready(h.run_auto());
    let mode =
        |path: &std::path::Path| fs::metadata(path).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(&h.updates_dir()), 0o700);
    assert_eq!(mode(update.path().parent().expect("pai")), 0o700);
    assert_eq!(mode(update.path()), 0o600);
}

// --- Proxy -------------------------------------------------------------------------------------

#[test]
fn requisicao_passa_pelo_proxy_configurado() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    let proxy = TcpListener::bind("127.0.0.1:0").expect("bind");
    let proxy_addr = proxy.local_addr().expect("addr");
    let seen = std::thread::spawn(move || {
        let (mut stream, _) = proxy.accept().expect("accept");
        let mut line = String::new();
        BufReader::new(stream.try_clone().expect("clone"))
            .read_line(&mut line)
            .expect("linha");
        let _ = stream.write_all(
            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        line
    });
    let origin = url::Url::parse("http://alvo.invalid:8080").expect("origem");
    let transport = updater_net::ureq_transport::UreqTransport::new(
        &updater_net::ureq_transport::TransportConfig::default()
            .with_proxy(Some(format!("http://{proxy_addr}")))
            .with_test_origin(origin),
    )
    .expect("transporte");

    let result = transport.get(updater::constants::MANIFEST_URL, None, 1024);

    assert!(result.is_err());
    let line = seen.join().expect("thread do proxy");
    assert!(
        line.starts_with("CONNECT alvo.invalid:8080"),
        "o proxy deveria receber CONNECT, recebeu: {line}"
    );
}
