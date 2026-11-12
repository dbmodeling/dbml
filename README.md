# DB Modeling

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Visual relational database modeling tool powered by **DBML (Database Markup Language)** — available as a **Native Desktop App (GPUI)**, **Interactive Web CLI**, and **VS Code Extension**, all sharing the same high-performance Rust core (compiled to native code and WebAssembly).

<p align="center">
  <img src="docs/img/canvas-escuro.png" alt="Canvas in dark theme" width="49%" />
  <img src="docs/img/canvas-claro.png" alt="Canvas in light theme" width="49%" />
</p>
<p align="center">
  <img src="docs/img/editor-dbml.png" alt="Integrated DBML editor with autocomplete and visual inspector" width="99%" />
</p>

## Highlights

- **Strict DBML as the single source of truth** — `.dbml` files remain 100% pure and compatible with `dbdiagram.io`, without polluting the schema with proprietary comments. Positions and colors are saved in a deterministic sidecar (`<file>.dbml.layout.json` with sorted keys, friendly to Git diffs).
- **1:1 Experience across Desktop, Browser, and VS Code** — Same visual workbench (Topbar, Sidebar with Project/Schema/Problems, Interactive Canvas, Inspector, integrated DBML Editor with syntax highlighting & autocomplete, Command Palette `Ctrl+K`, and Keyboard Shortcuts `Ctrl+/`).
- **Bidirectional real-time editing** — Edit in the DBML text editor or directly through the visual interface (create/rename/delete tables, columns, primary keys, `NOT NULL`/`UNIQUE` flags, groups, and drag between columns to create foreign key `Ref:` relations). All visual actions generate minimal textual patches (`dbml-edit`) that preserve comments and formatting.
- **Deterministic Layout & Orthogonal Routing** — Sugiyama hierarchical layout (`layout` crate) and orthogonal relation lines with rounded corners, cardinality badges (`1` and `*`), and obstacle avoidance.
- **High-resolution Export** — Export diagrams to **PNG (2x)** and **SVG** with optional title, color legend, and light/dark theme support.

---

## Architecture (Multi-Target)

```text
                        ┌──────────────────────────────────────────────┐
                        │               Rust Core Crates               │
                        │  schema · dbml-syntax (Rowan CST) · layout   │
                        │  dbml-analysis (lint/complete) · dbml-edit   │
                        └───────┬──────────────────────────────┬───────┘
                                │ Native Rust                  │ WebAssembly
                                ▼                              ▼
                     ┌─────────────────────┐        ┌─────────────────────┐
                     │     crates/app      │        │  crates/dbml-wasm   │
                     │  Desktop App (GPUI) │        └──────────┬──────────┘
                     └─────────────────────┘                   │
                                                               ▼
                                                    ┌─────────────────────┐
                                                    │ packages/canvas-web │
                                                    │ Shared Web Workbench│
                                                    └────┬───────────┬────┘
                                                         │           │
                                           ┌─────────────┘           └─────────────┐
                                           ▼                                       ▼
                                ┌─────────────────────┐                 ┌─────────────────────┐
                                │     crates/cli      │                 │  vscode-extension   │
                                │  HTTP + WS Server   │                 │  Custom Editor VSIX │
                                └─────────────────────┘                 └─────────────────────┘
```

---

## Running & Building

### 1. Synchronized Build (Wasm + Web + CLI + VS Code + Desktop)

Whenever you build the full workspace or update core/web components, run:

```bash
./scripts/sync-build.sh
```

For an optimized release build:

```bash
./scripts/sync-build.sh --release
```

### 2. Native Desktop App (`crates/app`)

#### System Dependencies (Ubuntu / Debian)

```bash
sudo apt update && sudo apt install -y \
  build-essential pkg-config cmake \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libfontconfig1-dev libssl-dev
```

#### Running Desktop App

```bash
cargo run -p app --release
# Or open a file directly:
cargo run -p app --release -- path/to/schema.dbml
```

### 3. Interactive Web CLI (`crates/cli`)

Starts a local HTTP + WebSocket server with live reload and two-way persistence for `.dbml` and `.dbml.layout.json`:

```bash
cargo run -p dbml-cli -- path/to/schema.dbml
# Options: --port 3000 --no-open --readonly --theme dark
```

### 4. VS Code Extension (`vscode-extension/`)

```bash
cd vscode-extension
npm run compile
# Press F5 in VS Code or package a .vsix with:
npx @vscode/vsce package
```

---

## Native Installers & Packaging

The `scripts/` folder includes automated packaging scripts for all desktop platforms:

- **Debian / Ubuntu (`.deb`):** `./scripts/build-deb.sh`
- **Fedora / RHEL (`.rpm`):** `./scripts/build-fedora.sh`
- **macOS Apple Silicon / Intel (`.dmg`):** `./scripts/build-macos.sh`
- **Windows (`.exe` / `.msi`):** `./scripts/build-windows.sh`

---

## Code Signing & Privacy

Free code signing provided by [SignPath.io](https://about.signpath.io/), using a certificate by [SignPath Foundation](https://signpath.org/).

DB Modeling runs locally and does not collect telemetry or send your `.dbml` schemas to external servers. See [SECURITY.md](SECURITY.md) for our signing policy and vulnerability reporting instructions.

---

## License

Licensed under the [MIT License](LICENSE).
