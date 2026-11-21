//! Superfície pública das edições de texto inteiro.
//!
//! Cada função aqui delega à implementação do módulo correspondente, mas preserva o
//! terminador de linha predominante do documento (ver [`crate::eol`]): um arquivo CRLF
//! nunca volta com `\n` solto. Documentos com terminadores mistos voltam normalizados
//! para o terminador predominante.

use crate::eol::eol_fn;
use crate::format as formatting;
use crate::model::RefUpdate;
use crate::{ColumnPreset, enums, groups, indexes, model};
use schema::IndexDef;

eol_fn! {
    /// Formats a DBML document with consistent indentation while preserving comments.
    format() -> String => formatting::format
}

eol_fn! {
    /// Appends a new `Enum` block named `name`.
    add_enum(name: &str) -> String => enums::add_enum
}
eol_fn! {
    /// Removes the `Enum` block `name`.
    delete_enum(name: &str) -> String => enums::delete_enum
}
eol_fn! {
    /// Renames an enum and every column type that refers to it.
    rename_enum(old_name: &str, new_name: &str) -> String => enums::rename_enum
}
eol_fn! {
    /// Appends `value` to the enum `enum_name`.
    add_enum_value(enum_name: &str, value: &str) -> Option<String> => enums::add_enum_value
}
eol_fn! {
    /// Removes `value` from the enum `enum_name`.
    delete_enum_value(enum_name: &str, value: &str) -> Option<String> => enums::delete_enum_value
}
eol_fn! {
    /// Renames a value of the enum `enum_name`.
    rename_enum_value(enum_name: &str, old_val: &str, new_val: &str) -> Option<String>
        => enums::rename_enum_value
}
eol_fn! {
    /// Moves a value of the enum `enum_name` one position up or down.
    reorder_enum_value(enum_name: &str, value: &str, up: bool) -> Option<String>
        => enums::reorder_enum_value
}

eol_fn! {
    /// Puts `tables` into the `TableGroup` `name`, creating it when needed.
    group_tables(tables: &[&str], name: &str) -> String => groups::group_tables
}
eol_fn! {
    /// Removes `table` from every `TableGroup`.
    ungroup_table(table: &str) -> String => groups::ungroup_table
}
eol_fn! {
    /// Sets or clears the `headercolor` of `table`.
    set_table_color(table: &str, color: Option<&str>) -> Option<String> => groups::set_table_color
}

eol_fn! {
    /// Sets or clears the `default: ...` attribute on `table.col`.
    set_column_default(table: &str, col: &str, default_val: Option<&str>) -> Option<String>
        => indexes::set_column_default
}
eol_fn! {
    /// Sets or clears the ``check: `...` `` attribute on `table.col`.
    set_column_check(table: &str, col: &str, check_expr: Option<&str>) -> Option<String>
        => indexes::set_column_check
}
eol_fn! {
    /// Adds `index` to the `indexes { }` block of `table`.
    add_index(table: &str, index: &IndexDef) -> Option<String> => indexes::add_index
}
eol_fn! {
    /// Replaces the `index_idx`-th entry of the `indexes { }` block of `table`.
    update_index(table: &str, index_idx: usize, index: &IndexDef) -> Option<String>
        => indexes::update_index
}
eol_fn! {
    /// Removes the `index_idx`-th entry of the `indexes { }` block of `table`.
    delete_index(table: &str, index_idx: usize) -> Option<String> => indexes::delete_index
}

eol_fn! {
    /// Appends a new table named `name`.
    add_table(name: &str) -> String => model::add_table
}
eol_fn! {
    /// Removes the table `name` together with its refs.
    delete_table(name: &str) -> String => model::delete_table
}
eol_fn! {
    /// Renames a table and every reference to it.
    rename_table(old: &str, new: &str) -> String => model::rename_table
}
eol_fn! {
    /// Adds the still-missing columns of `preset` to `table`.
    add_column_preset_text(table: &str, preset: ColumnPreset) -> Option<String>
        => model::add_column_preset
}
eol_fn! {
    /// Appends column `name` of type `ty` to `table`.
    add_column_text(table: &str, name: &str, ty: &str) -> Option<String> => model::add_column
}
eol_fn! {
    /// Removes `table.col` together with its refs.
    delete_column(table: &str, col: &str) -> Option<String> => model::delete_column
}
eol_fn! {
    /// Renames `table.old` and every reference to it.
    rename_column(table: &str, old: &str, new: &str) -> Option<String> => model::rename_column
}
eol_fn! {
    /// Changes the type of `table.col`.
    set_column_type(table: &str, col: &str, ty: &str) -> Option<String> => model::set_column_type
}
eol_fn! {
    /// Turns the setting `flag` (`pk`, `unique`, `not null`, ...) of `table.col` on or off.
    set_column_flag(table: &str, col: &str, flag: &str, on: bool) -> Option<String>
        => model::set_column_flag
}
eol_fn! {
    /// Appends a `Ref:` line from `from` to `to`.
    add_ref(from: (&str, &str), to: (&str, &str)) -> String => model::add_ref
}
eol_fn! {
    /// Links `source_table` to `target_table`: adds an FK column to `target_table` and its `Ref:`.
    link_tables(source_table: &str, target_table: &str) -> String => model::link_tables
}
eol_fn! {
    /// Removes the ref between `from` and `to`.
    delete_ref(from: (&str, &str), to: (&str, &str)) -> String => model::delete_ref
}
eol_fn! {
    /// Rewrites the ref between `from` and `to` according to `update`.
    update_ref(from: (&str, &str), to: (&str, &str), update: RefUpdate) -> String
        => model::update_ref
}
