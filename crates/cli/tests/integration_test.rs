#[tokio::test]
async fn test_state_creation_and_atomic_write() {
    let tmp_dir = std::env::temp_dir();
    let test_file = tmp_dir.join(format!("test_schema_{}.dbml", std::process::id()));

    // Limpeza prévia
    let _ = tokio::fs::remove_file(&test_file).await;

    // Criação do estado e do arquivo inicial
    let state = dbml_cli::state::AppState::new(test_file.clone())
        .await
        .unwrap();

    let initial = state.doc.read().await.content.clone();
    assert!(initial.contains("Table users"));

    // Gravação atômica de nova versão
    let updated = format!("{initial}\nTable logs {{ id int }}\n");
    state.save_content(updated.clone()).await.unwrap();

    let on_disk = tokio::fs::read_to_string(&test_file).await.unwrap();
    assert_eq!(on_disk, updated);

    // Gravação de layout
    let layout_json = r#"{"tables":{"users":{"x":100,"y":100}}}"#.to_string();
    state.save_layout(layout_json.clone()).await.unwrap();

    let lp = state.doc.read().await.layout_path.clone();
    let layout_on_disk = tokio::fs::read_to_string(&lp).await.unwrap();
    assert_eq!(layout_on_disk, layout_json);

    // Limpeza
    let _ = tokio::fs::remove_file(&test_file).await;
    let _ = tokio::fs::remove_file(&lp).await;
}

#[tokio::test]
async fn test_hash_calculation_and_suppression() {
    let text1 = "Table users { id int }";
    let text2 = "Table users { id int }";
    let text3 = "Table users { id bigint }";

    assert_eq!(
        dbml_cli::state::compute_hash(text1),
        dbml_cli::state::compute_hash(text2)
    );
    assert_ne!(
        dbml_cli::state::compute_hash(text1),
        dbml_cli::state::compute_hash(text3)
    );
}

#[tokio::test]
async fn test_switch_file_rejects_outside_workspace() {
    let base = std::env::temp_dir().join(format!("dbml_ws_test_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&base).await;
    tokio::fs::create_dir_all(&base).await.unwrap();

    let inside = base.join("inside.dbml");
    let outside = std::env::temp_dir().join(format!("dbml_outside_{}.dbml", std::process::id()));
    tokio::fs::write(&inside, "Table a { id int [pk] }\n")
        .await
        .unwrap();
    tokio::fs::write(&outside, "Table secret { id int [pk] }\n")
        .await
        .unwrap();

    let state = dbml_cli::state::AppState::new(base.clone()).await.unwrap();
    assert!(state.switch_file(inside.clone()).await.is_ok());
    assert!(state.switch_file(outside.clone()).await.is_err());

    let _ = tokio::fs::remove_dir_all(&base).await;
    let _ = tokio::fs::remove_file(&outside).await;
}

#[tokio::test]
async fn test_switch_file_rejects_non_dbml_and_symlink_escape() {
    let root = std::env::temp_dir().join(format!("dbml_ws_sec_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&root).await;
    let base = root.join("ws");
    let outside = root.join("outside");
    tokio::fs::create_dir_all(&base).await.unwrap();
    tokio::fs::create_dir_all(&outside).await.unwrap();

    tokio::fs::write(base.join("a.dbml"), "Table a { id int [pk] }\n")
        .await
        .unwrap();
    tokio::fs::write(base.join(".env"), "SECRET=1\n")
        .await
        .unwrap();
    tokio::fs::write(outside.join("x.dbml"), "Table x { id int [pk] }\n")
        .await
        .unwrap();

    let state = dbml_cli::state::AppState::new(base.clone()).await.unwrap();
    assert!(state.switch_file(base.join(".env")).await.is_err());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("x.dbml"), base.join("link.dbml")).unwrap();
        assert!(state.switch_file(base.join("link.dbml")).await.is_err());
    }

    let _ = tokio::fs::remove_dir_all(&root).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switch_file_and_save_content_never_mix_documents() {
    let dir = std::env::temp_dir().join(format!("dbml_switch_race_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let a = dir.join("a.dbml");
    let b = dir.join("b.dbml");
    tokio::fs::write(&a, "Table a { id int }\n").await.unwrap();
    tokio::fs::write(&b, "Table b { id int }\n").await.unwrap();
    let state = dbml_cli::state::AppState::new(dir.clone()).await.unwrap();
    let (a, b) = (
        tokio::fs::canonicalize(&a).await.unwrap(),
        tokio::fs::canonicalize(&b).await.unwrap(),
    );

    let saver = {
        let state = state.clone();
        tokio::spawn(async move {
            for i in 0..150 {
                state
                    .save_content(format!("Table t{i} {{ id int }}\n"))
                    .await
                    .unwrap();
            }
        })
    };
    let switcher = {
        let state = state.clone();
        let (a, b) = (a.clone(), b.clone());
        tokio::spawn(async move {
            for i in 0..150 {
                let target = if i % 2 == 0 { b.clone() } else { a.clone() };
                state.switch_file(target).await.unwrap();
            }
        })
    };
    saver.await.unwrap();
    switcher.await.unwrap();

    // Em repouso, memória, hash e disco do documento ativo têm de concordar.
    let doc = state.doc.read().await.clone();
    let on_disk = tokio::fs::read_to_string(&doc.file_path).await.unwrap();
    assert_eq!(doc.content, on_disk);
    assert_eq!(doc.last_saved_hash, dbml_cli::state::compute_hash(&on_disk));
    assert_eq!(
        doc.layout_path.to_string_lossy(),
        format!("{}.layout.json", doc.file_path.display())
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn failed_write_leaves_memory_and_hash_untouched() {
    let dir = std::env::temp_dir().join(format!("dbml_write_fail_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join("schema.dbml");
    tokio::fs::write(&file, "Table a { id int }\n")
        .await
        .unwrap();
    let state = dbml_cli::state::AppState::new(file).await.unwrap();
    let before = state.doc.read().await.clone();

    // Simula falha de gravação: a pasta do documento some.
    tokio::fs::remove_dir_all(&dir).await.unwrap();
    assert!(
        state
            .save_content("Table z { id int }\n".into())
            .await
            .is_err()
    );
    assert!(state.save_layout("{}".into()).await.is_err());

    let after = state.doc.read().await.clone();
    assert_eq!(after.content, before.content);
    assert_eq!(after.last_saved_hash, before.last_saved_hash);
    assert_eq!(after.layout, before.layout);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_the_caller_does_not_abort_a_write() {
    let dir = std::env::temp_dir().join(format!("dbml_write_cancel_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join("schema.dbml");
    tokio::fs::write(&file, "Table a { id int }\n")
        .await
        .unwrap();
    let state = dbml_cli::state::AppState::new(file.clone()).await.unwrap();

    for i in 0..50 {
        let task = {
            let state = state.clone();
            tokio::spawn(async move {
                let _ = state
                    .save_content(format!("Table t{i} {{ id int }}\n"))
                    .await;
            })
        };
        tokio::task::yield_now().await;
        task.abort();
        let _ = task.await;
        // A escrita é concluída por inteiro mesmo com o chamador cancelado.
        let doc = state.doc.read().await.clone();
        let on_disk = tokio::fs::read_to_string(&file).await.unwrap();
        assert_eq!(on_disk, doc.content);
        assert_eq!(doc.last_saved_hash, dbml_cli::state::compute_hash(&on_disk));
    }
    let mut leftovers = 0;
    let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
    while let Some(e) = entries.next_entry().await.unwrap() {
        if e.file_name().to_string_lossy().contains(".tmp-") {
            leftovers += 1;
        }
    }
    assert_eq!(leftovers, 0, "sobrou arquivo temporário");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

struct Doc {
    dir: std::path::PathBuf,
    file: std::path::PathBuf,
    state: dbml_cli::state::AppState,
    name: String,
}

impl Doc {
    async fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("dbml_{tag}_{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let file = dir.join("schema.dbml");
        tokio::fs::write(&file, "Table a { id int }\n")
            .await
            .unwrap();
        let state = dbml_cli::state::AppState::new(file.clone()).await.unwrap();
        let name = state
            .doc
            .read()
            .await
            .file_path
            .to_string_lossy()
            .into_owned();
        Self {
            dir,
            file,
            state,
            name,
        }
    }

    async fn content(&self) -> String {
        self.state.doc.read().await.content.clone()
    }

    async fn disk(&self) -> String {
        tokio::fs::read_to_string(&self.file).await.unwrap()
    }

    /// Insere `text` no início, partindo do conteúdo atual em memória.
    async fn prepend(&self, text: &str) {
        let source = self.content().await;
        self.state
            .apply_patch(&self.name, &source, 0, 0, text)
            .await
            .unwrap();
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn patches_are_debounced_into_a_single_delayed_write() {
    let d = Doc::new("debounce").await;
    for i in 0..20 {
        d.prepend(&format!("// {i}\n")).await;
    }
    // Nada foi para o disco ainda; a memória já tem as 20 edições.
    assert_eq!(d.disk().await, "Table a { id int }\n");
    assert!(d.content().await.starts_with("// 19\n"));
    assert!(d.state.doc.read().await.dirty);

    tokio::time::sleep(dbml_cli::state::WRITE_DEBOUNCE * 4).await;
    assert_eq!(d.disk().await, d.content().await);
    assert!(!d.state.doc.read().await.dirty);
    let doc = d.state.doc.read().await.clone();
    assert_eq!(
        doc.last_saved_hash,
        dbml_cli::state::compute_hash(&doc.content)
    );
}

#[tokio::test]
async fn pending_patches_edit_the_buffer_in_place() {
    let d = Doc::new("inplace").await;
    // O primeiro patch carrega o texto do disco; os seguintes devem editar o mesmo buffer.
    d.prepend("// um\n").await;
    let before = d.state.doc.read().await.content.as_ptr();
    let source = d.content().await;
    d.state
        .apply_patch(&d.name, &source, 3, 5, "XX") // mesmo tamanho: não realoca
        .await
        .unwrap();
    let after = d.state.doc.read().await.content.as_ptr();
    assert_eq!(
        before, after,
        "o documento foi clonado em vez de editado no lugar"
    );
    assert!(d.content().await.starts_with("// XX\n"));

    // Patch inválido não corrompe nem descarta o documento.
    let source = d.content().await;
    assert!(
        d.state
            .apply_patch(&d.name, &source, 5, 3, "x")
            .await
            .is_err()
    );
    assert!(
        d.state
            .apply_patch(&d.name, "outra versão", 0, 0, "x")
            .await
            .is_err()
    );
    assert_eq!(d.content().await, source);
}

#[tokio::test]
async fn startup_removes_only_stale_atomic_write_leftovers() {
    use std::time::{Duration, SystemTime};
    let dir = std::env::temp_dir().join(format!("dbml_stale_tmp_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let old = SystemTime::now() - Duration::from_secs(600);
    let touch_old = |name: &str| {
        let path = dir.join(name);
        std::fs::write(&path, "x").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        path
    };
    let stale_doc = touch_old(".schema.dbml.tmp-4242-7");
    let stale_layout = touch_old(".schema.dbml.layout.json.tmp-4242-8");
    // Não casam com o padrão de `atomic_write`: ficam.
    let user_hidden = touch_old(".notes.tmp-1-2");
    let user_file = touch_old("backup.tmp-4242-7");
    let wrong_pid = touch_old(".schema.dbml.tmp-abc-7");
    // Recente: pode ser uma gravação em andamento de outro processo.
    let fresh = dir.join(".schema.dbml.tmp-4242-9");
    std::fs::write(&fresh, "x").unwrap();
    let schema = dir.join("schema.dbml");
    std::fs::write(&schema, "Table a { id int }\n").unwrap();

    let removed = dbml_cli::state::clean_stale_temp_files(&dir, Duration::from_secs(60)).await;
    assert_eq!(removed, 2);
    assert!(!stale_doc.exists() && !stale_layout.exists());
    assert!(user_hidden.exists() && user_file.exists() && wrong_pid.exists());
    assert!(fresh.exists() && schema.exists());

    // AppState::new também limpa a pasta do projeto (com a idade mínima padrão).
    let stale_again = touch_old(".schema.dbml.tmp-1-1");
    dbml_cli::state::AppState::new(dir.clone()).await.unwrap();
    assert!(!stale_again.exists());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn watcher_reports_only_external_changes_to_the_active_file() {
    use std::time::Duration;
    let d = Doc::new("watcher_filter").await;
    dbml_cli::watcher::start_file_watcher(d.state.clone()).unwrap();
    let mut rx = d.state.tx.subscribe();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Outro arquivo da pasta e um temporário de gravação atômica: nenhuma mensagem.
    tokio::fs::write(d.dir.join("other.dbml"), "Table o { id int }\n")
        .await
        .unwrap();
    tokio::fs::write(d.dir.join(".schema.dbml.tmp-1-1"), "x")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(rx.try_recv().is_err(), "evento irrelevante gerou mensagem");

    // Edição externa do arquivo ativo: mensagem external_change com o novo texto.
    tokio::fs::write(&d.file, "Table editada { id int }\n")
        .await
        .unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("watcher não reagiu")
        .unwrap();
    assert!(
        msg.contains("external_change") && msg.contains("editada"),
        "{msg}"
    );

    // A própria gravação (flush de um patch) não vira external_change.
    let source = d.content().await;
    d.state
        .apply_patch(&d.name, &source, 0, 0, "// nosso\n")
        .await
        .unwrap();
    d.state.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    while let Ok(msg) = rx.try_recv() {
        assert!(!msg.contains("external_change"), "{msg}");
    }
}

#[tokio::test]
async fn save_document_flushes_pending_edits() {
    let d = Doc::new("flush_save").await;
    d.prepend("// editado\n").await;
    let expected = d.content().await;
    assert_ne!(d.disk().await, expected);
    d.state.verify_document(&d.name, &expected).await.unwrap();
    assert_eq!(d.disk().await, expected);
}

#[tokio::test]
async fn switching_files_flushes_pending_edits() {
    let d = Doc::new("flush_switch").await;
    let other = d.dir.join("other.dbml");
    tokio::fs::write(&other, "Table o { id int }\n")
        .await
        .unwrap();
    // `Doc::new` abriu um arquivo único; reabre a pasta para poder trocar de arquivo.
    let state = dbml_cli::state::AppState::new(d.dir.clone()).await.unwrap();
    let active = state.doc.read().await.file_path.clone();
    let source = state.doc.read().await.content.clone();
    state
        .apply_patch(&active.to_string_lossy(), &source, 0, 0, "// pendente\n")
        .await
        .unwrap();
    let target = if active == other {
        d.file.clone()
    } else {
        other.clone()
    };
    state.switch_file(target).await.unwrap();
    let saved = tokio::fs::read_to_string(&active).await.unwrap();
    assert!(saved.starts_with("// pendente\n"), "{saved}");
}

#[tokio::test]
async fn shutdown_flushes_pending_edits() {
    let d = Doc::new("flush_shutdown").await;
    d.prepend("// antes de sair\n").await;
    assert_ne!(d.disk().await, d.content().await);
    d.state.shutdown(std::time::Duration::from_secs(1)).await;
    assert_eq!(d.disk().await, d.content().await);
    assert!(d.disk().await.starts_with("// antes de sair\n"));
}

#[tokio::test]
async fn stale_patch_is_still_rejected_while_edits_are_pending() {
    let d = Doc::new("stale_pending").await;
    let original = d.content().await;
    d.prepend("// um\n").await;
    let err = d
        .state
        .apply_patch(&d.name, &original, 0, 0, "// velho\n")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("changed"), "{err}");
    assert!(d.content().await.starts_with("// um\n"));
}

#[tokio::test]
async fn concurrent_patches_never_lose_an_edit() {
    let dir = std::env::temp_dir().join(format!("dbml_patch_race_{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join("schema.dbml");
    tokio::fs::write(&file, "Table a { id int }\n")
        .await
        .unwrap();
    let state = dbml_cli::state::AppState::new(file.clone()).await.unwrap();
    let source = state.doc.read().await.content.clone();
    let expected_file = state
        .doc
        .read()
        .await
        .file_path
        .to_string_lossy()
        .into_owned();

    // Dois clientes partem da mesma versão: só um pode vencer, o outro recebe erro de conflito.
    let (r1, r2) = tokio::join!(
        state.apply_patch(&expected_file, &source, 0, 0, "// um\n"),
        state.apply_patch(&expected_file, &source, 0, 0, "// dois\n"),
    );
    assert_eq!(
        u8::from(r1.is_ok()) + u8::from(r2.is_ok()),
        1,
        "exatamente um patch vence: {r1:?} / {r2:?}"
    );
    state.flush().await.unwrap();
    let on_disk = tokio::fs::read_to_string(&file).await.unwrap();
    assert_eq!(on_disk, *state.doc.read().await.content);
    assert!(on_disk.starts_with("// um\n") ^ on_disk.starts_with("// dois\n"));

    // Com a versão atual o próximo patch passa.
    let current = on_disk.clone();
    state
        .apply_patch(&expected_file, &current, 0, 0, "// tres\n")
        .await
        .unwrap();
    // Arquivo ativo diferente é recusado.
    assert!(
        state
            .apply_patch("/outro.dbml", &current, 0, 0, "x")
            .await
            .is_err()
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
