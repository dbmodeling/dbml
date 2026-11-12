# Agent & Maintenance Guide — DB Modeling

This repository hosts **DB Modeling**, a visual relational database modeling tool based on **DBML** (`Database Markup Language`) with **three main targets** that share the same Rust core and visual identity:

1. **Desktop App (`crates/app`)** — Native GPUI application (Windows, Linux, macOS).
2. **CLI Web (`crates/cli`)** — HTTP + WebSocket server with an interactive web canvas (`dbml` binary).
3. **VS Code Extension (`vscode-extension/`)** — Native extension with Custom Editor, Sidebar, and embedded Webview.

And the shared bridge/UI packages:
- **`crates/dbml-wasm`** — WebAssembly bindings exposing the Rust core (`dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`, `schema`) to the browser and VS Code.
- **`packages/canvas-web`** — Complete TypeScript/HTML5 Canvas workbench shared between the **CLI Web** and the **VS Code Extension**.

---

## 1. Golden Rule: Parity & Synchronization Across the 3 Targets

Whenever you implement a feature, fix a bug, change keyboard shortcuts, modify colors/themes, alter layout rules, or adjust DBML editing commands, **MANDATORILY check the impact across all 3 targets**:

| Layer / Feature | Where it lives in Desktop (GPUI) | Where it lives in Web (CLI & VS Code) |
| :--- | :--- | :--- |
| **Parser, CST & AST** | `crates/dbml-syntax` (incl. `unquote`, `child_tokens`, `descendant_tokens`), `crates/schema` | Shared via `crates/dbml-wasm` |
| **Lint, Diagnostics & Autocomplete** | `crates/dbml-analysis` | Shared via `crates/dbml-wasm` (`analyze`, `complete`, `format_source`) |
| **Structural Editing (Text Patches)** | `crates/dbml-edit` | Shared via `crates/dbml-wasm` (`apply_command`, `add_group`, etc.) |
| **Auto-Layout, Routing & Card Geometry** | `crates/layout` (`CARD_W`, `HEADER_H`, `ROW_H`, `GROUP_PAD`, `TABLE_COLORS`, `compute_diagram_positions`) | Shared via `crates/dbml-wasm` (`compute_layout`, `build_scene`) |
| **Canvas (Render, Pan/Zoom, Hit-test)** | `crates/app/src/canvas.rs` + `crates/app/src/canvas_geom.rs` | `packages/canvas-web/src/canvas-view.ts` + `renderer.ts` |
| **Workbench (Topbar, Sidebar, Inspector, Statusbar)** | `crates/app/src/shell/mod.rs` + `crates/app/src/shell/panels.rs` | `packages/canvas-web/src/workbench.ts` + `panels.ts` |
| **Shortcuts & Command Palette** | `crates/app/src/actions.rs` | `packages/canvas-web/src/actions.ts` |
| **Visual Tokens & Palette (8 Colors)** | `crates/app/src/glass.rs` | `packages/canvas-web/src/glass.ts` |
| **Persistence & Sidecar (`.layout.json`)** | `crates/app/src/shell/mod.rs`, `session.rs`, `settings.rs` (atomic writes) | `crates/cli/src/server.rs` (CLI) / `vscode-extension/src/customEditor.ts` (VS Code) |

### Checklist Before Finishing Any Task:
1. **Changed Rust core (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`)?**
   - Check if `crates/dbml-wasm/src/lib.rs` needs to expose the new capability or updated DTO.
   - Recompile Wasm and sync web packages by running `./scripts/sync-build.sh`.
2. **Changed UI, interaction, Inspector, Sidebar, or Canvas in Desktop (`crates/app`)?**
   - Port the exact same behavior and visual design to `packages/canvas-web` (or vice versa) to maintain 1:1 parity.
3. **Changed `packages/canvas-web` or `crates/dbml-wasm`?**
   - **MANDATORY:** Run `./scripts/sync-build.sh` so the static bundle in `crates/cli/web/` (embedded into the CLI binary via `rust-embed`) and the bundle in `vscode-extension/dist/` are updated!

---

## 2. Build & Synchronization Pipeline (`./scripts/sync-build.sh`)

To prevent the CLI or VS Code Extension from running outdated code after changes in Rust or `canvas-web`, use the unified script:

```bash
./scripts/sync-build.sh
```

What this script does in order:
1. Runs `cargo test` across the core crates (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`, `dbml-wasm`).
2. Compiles `crates/dbml-wasm` directly into `packages/canvas-web/src/wasm` using `wasm-pack build --target web` (or `cargo build --target wasm32-unknown-unknown` + `wasm-bindgen --target web` when `wasm-pack` is not installed).
3. Builds the web app (`npm run build:app` inside `packages/canvas-web`) and syncs `dist-app/` into `crates/cli/web/`.
4. Builds the library (`npm run build` inside `packages/canvas-web`) and packages the VS Code extension (`npm run compile` inside `vscode-extension/`).
5. Verifies the compilation of the `cli` and `app` crates.

---

## 3. Non-Negotiable Architectural Principles

1. **Strict DBML as the Single Source of Truth (ADR-0002 & ADR-0004):**
   - Never persist visual state (X/Y positions, table colors, group colors) inside `.dbml` comments or custom syntax.
   - All visual metadata goes into the sidecar `<file>.dbml.layout.json` with keys sorted deterministically (`BTreeMap` in Rust / sorted keys in TS).
   - Every visual edit on the Canvas or Inspector (renaming a table, changing a column type, creating an FK relation, grouping tables) **must** generate a minimal textual patch via `dbml-edit` (`EditCommand` -> `apply`), preserving user comments and formatting.
2. **Core Free of UI Dependencies (ADR-0005):**
   - `schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`, and `dbml-wasm` must never depend on `gpui`, `tokio`, or OS APIs.
3. **Lossless Error Recovery (ADR-0003):**
   - The parser (`dbml-syntax`) must never panic on incomplete input while the user is typing in the editor.

---

## 4. Domain Documentation & Issue Tracker

For domain modeling skills, architecture, and local tickets:
- Domain and ADRs: see `docs/agents/domain.md` (`GLOSSARY.md` and `openspec/adr/`).
- Local issue tracker (`.scratch/`): see `docs/agents/issue-tracker.md` and `docs/agents/triage-labels.md`.
