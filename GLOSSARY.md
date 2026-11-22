# Glossary

## Bounded Contexts

Single context (`db_modeling`): visual modeling of relational schemas over DBML files, separated into core crates without UI dependencies (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `sql-import`, `layout`, `dbml-wasm`) and interactive targets (`app` in GPUI, `cli` in Rust + Wasm, `vscode-extension`, and `packages/canvas-web`).

## Ubiquitous Language

| Term | Type (`schema::`) | Meaning | Avoid |
| --- | --- | --- | --- |
| **Schema** | `Schema` | Set of tables, columns, relations, and groups derived from DBML text; never persisted directly (`docs/03-arquitetura.md`). | _Model_, _Database_, _Diagram_ |
| **Table** | `Table` | Relational entity with `name`, optional `schema` (`public.users`), `alias`, `columns`, `note`, and header settings (`header_color`). | _Entity_, _Box_ |
| **Column** | `Column` | Field of a table with `name`, `col_type`, flags (`is_pk`, `not_null`, `unique`, `increment`), `default`, `note`, and inline `refs`. | _Field_, _Attribute_, _Property_ |
| **Relation** | `Relation` | Explicit link (`Ref:`) or inline (`[ref: > ...]`) between two `Endpoint`s (`schema`, `table`, `column`) with `Cardinality` (`OneToOne`, `ManyToOne`, `OneToMany`, `ManyToMany`). | _Relationship_, _FK_, _Arrow_, _Edge_, _Link_ |
| **Group** | `TableGroup` | Named visual grouping of tables (`TableGroup name { ... }`). | _Cluster_, _Namespace_, _Module_ |
| **Sidecar** | `LayoutSidecar` (`app`/`canvas-web`) | Auxiliary file `<file>.dbml.layout.json` containing visual state that does not belong in strict DBML: positions `(x, y)` per table and colors (`table_colors`, `group_colors`). | _Metadata_, _Config_, _Cache_ |
| **CST** | `dbml_syntax::SyntaxNode` | Lossless concrete syntax tree (`rowan`) preserving comments, spaces, and tokens with errors (`docs/03-arquitetura.md`). | _AST_ |
| **Patch** | `dbml_edit::TextPatch` | Minimal range edit `[start..end) -> replacement` applied to the DBML text from a semantic canvas command (`EditCommand`). | _Diff_, _Mutation_ |
| **SQL import** | `sql_import::ImportOutcome` | Tolerant conversion from SQL DDL (`PostgreSQL`, `MySQL`, `SQLite`) into DBML text plus non-fatal `ImportWarning` diagnostics. | _Reverse-engineering_, _SQL migration_ |
| **Compare mode (Diff mode)** | `ToggleDiff` (`canvas-web`) / `ToggleDiffMode` (`app`) | Read-only mode that compares the open document (the head) against its base and paints the differences on the canvas with colors and markers (`+` added, `~` modified, `−` removed) plus a summary panel. It never edits text, writes patches, or enters the undo history. Shortcuts: `D` (Web/VS Code), `Ctrl+Alt+G` (Desktop); `n` / `Shift+N` jump to the next/previous change (Desktop). | _Patch_ (a patch edits; Compare mode never does) |
| **Base (HEAD)** | `DiffState::Ready { base }` | The text the head is compared against in Compare mode: the version of the open file at git `HEAD`. Fetched once from the host and kept in memory; the diff is recomputed on each edit without querying git again. Unavailable for untracked files, repos without commits, or files outside a repo. | _Original_, _Previous version_ |
| **Ghost card** | `renderGhostTable` / `ghostSlots` (`canvas-web`) | Translucent dashed card that represents a table or enum present in the base but removed in the head. It sits in free space below the diagram, is not selectable or draggable, and is included in "fit to screen" and in the minimap while Compare mode is active. | _Deleted card_, _Placeholder_ |
