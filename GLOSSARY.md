# Glossary

## Bounded Contexts

Single context (`db_modeling`): visual modeling of relational schemas over DBML files, separated into core crates without UI dependencies (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `sql-import`, `layout`, `updater`, `dbml-wasm`) and interactive targets (`app` in GPUI, `cli` in Rust + Wasm, `vscode-extension`, and `packages/canvas-web`).

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

## Auto-update do App Desktop

Termos do auto-update (só o App Desktop; ver [docs/auto-update.md](docs/auto-update.md) e o ADR 0010). Esta seção está em pt-BR, como o restante da documentação do projeto.

| Termo | Tipo / onde | Significado | Evitar |
| --- | --- | --- | --- |
| **Manifesto** | `updater::manifest::Manifest` (`latest.json` + `latest.json.sig`) | JSON estático e assinado (minisign) publicado junto da Release: `schema`, `version`, `notes_url` e, por plataforma (`windows-x86_64`, `macos-aarch64`, `linux-x86_64`), `url`, `sha256` e, no macOS assinado, `team_id`. O app só confia no manifesto cuja assinatura uma das duas chaves públicas embutidas valida. Esquema desconhecido é ignorado. | _Feed_, _appcast_, _endpoint_ |
| **Canal** | — (só `stable` na v1) | Linha de versões que o app segue. Na v1 existe um só, `stable`: a última Release que não é pré-release (`releases/latest/download`). Pré-release fica fora; não há seletor de canal. | _Quality_, _ring_ |
| **Razão de desativação** | `updater::policy::DisabledReason` | Motivo pelo qual o auto-update está desligado por causa do **ambiente**, não da preferência do usuário: `DBM_UPDATE_EXPLANATION`, arquivo `/usr/lib/db-modeling/update-explanation` dos pacotes, plataforma desconhecida, local do app indeterminado, Windows sob `%ProgramFiles%`, macOS em App Translocation, ou pasta sem escrita. Quando existe, nenhuma checagem (nem manual) roda e a UI mostra o texto no lugar da ação. Diferente de `auto_update = false`, que só desliga a checagem automática. | _Bloqueio_, _erro de update_ |
| **Atualização instalada aguardando reinício** | `UpdateStatus::Updated(versão)` / `UpdateState::pending()` | A versão nova já foi baixada, verificada e **instalada** nos arquivos; o processo em execução ainda é o antigo. A title bar mostra "Reiniciar para atualizar". Não existe estado "baixada mas não instalada". No Windows são os arquivos em `{app}\install` mais a flag `updates\versions.txt`, e a troca final é do helper. | _Update pendente de download_, _staged_ |
| **`DBM_UPDATE_EXPLANATION`** | variável de ambiente (`updater::constants::ENV_UPDATE_EXPLANATION`) | Se definida e não vazia, desativa o auto-update e seu valor vira o texto mostrado ao usuário (por exemplo, "atualize pelo gerenciador de pacotes"). É o override do usuário ou do empacotador e tem precedência sobre o arquivo `/usr/lib/db-modeling/update-explanation`. Equivale ao `ZED_UPDATE_EXPLANATION` do Zed. | _`DBM_UPDATE`_, _`DBM_NO_UPDATE`_ |
| **Helper (de atualização)** | crate `auto-update-helper` (`tools\auto_update_helper.exe`, só Windows) | Processo separado que troca os arquivos do app depois que ele sai: espera o PID, libera handles com o Restart Manager, move os arquivos antigos para `old\` e os de `install\` para o lugar, desfaz tudo em ordem inversa se um job falhar (e então não relança) e apaga `updates`, `install` e `old`. Roda com `--launch true` ("Reiniciar para atualizar") ou `--launch false` (ao sair). | _Updater_ (é o nome da crate pura), _installer_ |
| **Trava de atualização** | `updater::lock::UpdateLock` (`<cache>/db-modeling/updates/lock`) | Trava de arquivo exclusiva que o updater toma durante uma rodada, já que o app aceita várias instâncias. Quem não a obtém pula a rodada. | _Mutex_ (é de arquivo, não nomeado) |
| **Rodada** | `updater_net::round::run_round` | Uma passada completa da política até o artefato verificado: política, plataforma, trava, manifesto, assinatura, regra de versão, download e SHA-256. A instalação é um passo seguinte, feito pelo instalador da plataforma. | _Check_ (só a consulta ao manifesto) |
