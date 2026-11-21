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
