//! Toda edição estrutural sobre um documento CRLF deve devolver só CRLF (issue #53).

use dbml_edit::*;
use proptest::prelude::*;

const CRLF_DOC: &str = "// loja\r\nTable users {\r\n  id int [pk]\r\n  email text\r\n}\r\n\r\nTable posts {\r\n  id int [pk]\r\n  user_id int\r\n}\r\n\r\nEnum role {\r\n  admin\r\n}\r\n\r\nRef: posts.user_id > users.id\r\n";

fn only_crlf(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes
        .iter()
        .enumerate()
        .all(|(i, &b)| b != b'\n' || (i > 0 && bytes[i - 1] == b'\r'))
}

fn assert_crlf(label: &str, out: &str) {
    assert!(only_crlf(out), "{label} gerou LF puro: {out:?}");
    assert!(out.contains("\r\n"), "{label}: perdeu os CRLF");
}

#[test]
fn text_edits_keep_crlf() {
    let t = CRLF_DOC;
    assert_crlf("add_table", &add_table(t, "tags"));
    assert_crlf("delete_table", &delete_table(t, "posts"));
    assert_crlf("rename_table", &rename_table(t, "posts", "articles"));
    assert_crlf(
        "add_column",
        &add_column_text(t, "users", "age", "int").unwrap(),
    );
    assert_crlf(
        "delete_column",
        &delete_column(t, "users", "email").unwrap(),
    );
    assert_crlf(
        "rename_column",
        &rename_column(t, "users", "email", "mail").unwrap(),
    );
    assert_crlf(
        "set_type",
        &set_column_type(t, "users", "email", "varchar").unwrap(),
    );
    assert_crlf(
        "set_flag",
        &set_column_flag(t, "users", "email", "unique", true).unwrap(),
    );
    assert_crlf("add_ref", &add_ref(t, ("users", "id"), ("posts", "id")));
    assert_crlf("link_tables", &link_tables(t, "posts", "users"));
    assert_crlf(
        "delete_ref",
        &delete_ref(t, ("posts", "user_id"), ("users", "id")),
    );
    assert_crlf("add_enum", &add_enum(t, "status"));
    assert_crlf(
        "add_enum_value",
        &add_enum_value(t, "role", "user").unwrap(),
    );
    assert_crlf("group", &group_tables(t, &["users", "posts"], "core"));
    assert_crlf("color", &set_table_color(t, "users", Some("#fff")).unwrap());
    assert_crlf(
        "preset",
        &add_column_preset_text(t, "users", ColumnPreset::Timestamps).unwrap(),
    );
    assert_crlf("format", &format(t));
}

#[test]
fn patch_edit_keeps_crlf() {
    let add = |table: &str| Command::AddColumn {
        table: table.into(),
        name: "age".into(),
        ty: "int".into(),
    };
    let out = edit(CRLF_DOC, &add("users")).unwrap().apply(CRLF_DOC);
    assert_crlf("edit AddColumn", &out);
    let empty = "Table t {\r\n}\r\n";
    let out = edit(empty, &add("t")).unwrap().apply(empty);
    assert_crlf("edit AddColumn tabela vazia", &out);
}

#[test]
fn lf_documents_are_untouched() {
    let out = add_table("Table a {\n  id int\n}\n", "b");
    assert!(!out.contains('\r'));
}

proptest! {
    #[test]
    fn crlf_is_preserved_for_arbitrary_names(name in "[a-z][a-z0-9_]{0,10}", col in "[a-z][a-z0-9_]{0,10}") {
        let t = CRLF_DOC;
        prop_assert!(only_crlf(&add_table(t, &name)));
        prop_assert!(only_crlf(&add_column_text(t, "users", &col, "int").unwrap_or_else(|| t.to_string())));
        prop_assert!(only_crlf(&add_enum(t, &name)));
    }
}
