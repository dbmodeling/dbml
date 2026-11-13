<div align="center">

<img src="favicon.svg" alt="DB Modeling logo" width="72" height="72" />

# DB Modeling

**Visual relational database modeling in a fast, native desktop app, interactive web CLI, and VS Code extension. Built in Rust with GPUI and WebAssembly — no Electron, 100% local-first, and pure DBML as the single source of truth.**

[Download](https://github.com/dbmodeling/dbml/releases) · [Highlights](#highlights) · [Installation](#installation--usage) · [Running & Building](#running--building) · [Contributing](#contributing)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

</div>

<p align="center">
  <img src="docs/img/canvas-escuro.png" alt="Canvas in dark theme" width="49%" />
  <img src="docs/img/canvas-claro.png" alt="Canvas in light theme" width="49%" />
</p>
<p align="center">
  <img src="docs/img/editor-dbml.png" alt="Integrated DBML editor with autocomplete and visual inspector" width="99%" />
</p>

---

## Why DB Modeling

Most visual database modeling tools either lock your schemas inside a cloud web app or run inside a heavy Electron wrapper. **DB Modeling** renders its desktop UI natively on the GPU with **GPUI** and compiles the exact same Rust core to **WebAssembly** for the browser CLI and VS Code — keeping `.dbml` files 100% pure, local, and Git-friendly.

| Capability | DB Modeling | Cloud ERD Tools | Electron-based Tools |
| :--- | :--- | :--- | :--- |
| **Native desktop rendering** | Yes (GPUI, GPU-accelerated) | No (browser tab) | No (embedded Chromium) |
| **Runs offline / local-first** | Yes | Limited / requires account | Yes |
| **Pure `.dbml` source of truth** | Yes (`dbdiagram.io` compatible) | Often proprietary | Varies |
| **Git-friendly layout metadata** | Deterministic `.layout.json` | Cloud-only | Mixed into schema |
| **Desktop + Web CLI + VS Code** | Yes (shared Rust + Wasm core) | Web only | Desktop only |
| **Telemetry or external servers** | None | Yes | Varies |

---

## Highlights

- **Strict DBML as the single source of truth** — `.dbml` files stay 100% pure and compatible with `dbdiagram.io`. Positions and colors are saved in a deterministic `<file>.dbml.layout.json` sidecar with sorted keys for clean Git diffs.
- **1:1 Experience across Desktop, Browser, and VS Code** — Shared Rust core (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`) compiled to native GPUI and WebAssembly (`dbml-wasm` + `canvas-web`), with the same Topbar, Sidebar, Canvas, Inspector, DBML Editor, Command Palette (`Ctrl+K`), and Shortcuts (`Ctrl+/`).
- **Bidirectional real-time editing** — Edit in the DBML editor or visually on the canvas (tables, columns, `PK`/`NOT NULL`/`UNIQUE`, groups, drag-to-connect `Ref:` foreign keys). Visual edits generate minimal textual patches (`dbml-edit`) preserving comments and formatting.
- **Deterministic Layout & Orthogonal Routing** — Sugiyama hierarchical layout with rounded orthogonal relation lines, cardinality badges (`1` and `*`), and obstacle avoidance.
- **High-resolution Export** — Export diagrams to **PNG (2x)** and **SVG** with optional title, color legend, and light/dark themes.

---

## Installation & Usage

- **Windows:** Download `db-modeling-windows-setup.exe` from [GitHub Releases](https://github.com/dbmodeling/dbml/releases) and run the installer.
- **macOS:** Download the `.dmg` / `.pkg` installer from [GitHub Releases](https://github.com/dbmodeling/dbml/releases).
- **Linux (Ubuntu / Debian / Fedora):** Download the `.deb` or `.rpm` from [GitHub Releases](https://github.com/dbmodeling/dbml/releases), or run:
  ```bash
  curl -fsSL https://github.com/dbmodeling/dbml/releases/latest/download/db-modeling-install.sh | bash
  ```
- **Interactive Web CLI (`dbml`):** Start a local HTTP + WebSocket server with live reload and two-way persistence:
  ```bash
  dbml path/to/schema.dbml
  # Options: --port 3000 --no-open --readonly --theme dark
  ```
- **VS Code Extension:** Open any `.dbml` file using the **DB Modeling** custom editor (`vscode-extension/`).

---

## Running & Building

Sync and build all targets (Rust core, Wasm, Web CLI, VS Code extension, and Desktop app):

```bash
git clone https://github.com/dbmodeling/dbml.git
cd dbml
./scripts/sync-build.sh --release
```

Run individual targets directly:

```bash
# Native Desktop App (crates/app)
cargo run -p app --release -- path/to/schema.dbml

# Interactive Web CLI (crates/cli)
cargo run -p dbml-cli -- path/to/schema.dbml
```

- **Ubuntu / Debian dependencies:** `build-essential pkg-config cmake libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libfontconfig1-dev libssl-dev`
- **VS Code Extension:** `cd vscode-extension && npm run compile` (or `npx @vscode/vsce package` for `.vsix`)
- **Native Installers (`scripts/`):** `./scripts/build-deb.sh`, `./scripts/build-fedora.sh`, `./scripts/build-macos.sh`, `./scripts/build-windows.sh`

---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for the workspace layout, architecture rules, and CI checks (`cargo fmt`, `cargo clippy`, `cargo test`, and `./scripts/sync-build.sh`).

---

## Security & License

Free code signing provided by [SignPath.io](https://about.signpath.io/), using a certificate by [SignPath Foundation](https://signpath.org/). DB Modeling runs locally with zero telemetry — see [SECURITY.md](SECURITY.md) for details.

Licensed under the **[MIT License](LICENSE)**.
