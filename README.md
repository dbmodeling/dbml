<div align="center">

<img src="favicon.svg" alt="DB Modeling logo" width="72" height="72" />

# DB Modeling

**Visual relational database modeling in a fast, native desktop app, interactive web CLI, and VS Code extension. Built in Rust with GPUI and WebAssembly — no Electron, 100% local-first, and pure DBML as the single source of truth.**

[Download](https://github.com/dbmodeling/dbml/releases) · [Features](#features) · [Installation](#installation--usage) · [Build from Source](#build-from-source) · [Security](SECURITY.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

</div>

<p align="center">
  <img src="docs/img/canvas-escuro.png" alt="DB Modeling canvas in dark theme" width="49%" />
  <img src="docs/img/canvas-claro.png" alt="DB Modeling canvas in light theme" width="49%" />
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

## Features

* **Strict DBML as the Single Source of Truth:** `.dbml` files stay 100% pure and compatible with `dbdiagram.io`, without polluting your schema with proprietary comments. Positions and colors live in a deterministic `<file>.dbml.layout.json` sidecar with sorted keys for clean Git diffs.
* **Bidirectional Real-Time Editing:** Edit in the integrated DBML text editor or directly on the canvas and inspector — create, rename, or delete tables, columns, `PK`/`NOT NULL`/`UNIQUE` flags, table groups, and drag between columns to create `Ref:` foreign keys. Every visual edit applies a minimal textual patch (`dbml-edit`) that preserves your comments and formatting.
* **1:1 Parity Across 3 Targets:** Share the exact same workbench (Topbar, Project/Schema/Problems Sidebar, Interactive Canvas, Inspector, Command Palette `Ctrl+K`, and Shortcuts `Ctrl+/`) across the **Native Desktop App**, **Interactive Web CLI**, and **VS Code Extension**.
* **Deterministic Layout & Orthogonal Routing:** Automatic Sugiyama hierarchical layout (`layout` crate) and orthogonal relation lines with rounded corners, cardinality badges (`1` and `*`), and obstacle avoidance.
* **Lossless Parser & Smart Autocomplete:** Powered by a fault-tolerant Rowan CST parser (`dbml-syntax`) and static analyzer (`dbml-analysis`) with live diagnostics and context-aware completions while you type.
* **High-Resolution Export:** Export diagrams to **PNG (2x)** and **SVG** with optional title, color legend, and light/dark theme support.

---

## Installation & Usage

* **Windows:** Download the latest `db-modeling-windows-setup.exe` from [GitHub Releases](https://github.com/dbmodeling/dbml/releases) and run the installer.
* **macOS:** Download the `.dmg` / `.pkg` installer from [GitHub Releases](https://github.com/dbmodeling/dbml/releases), or run the universal installer script below.
* **Linux (Ubuntu / Debian / Fedora):** Download the `.deb` or `.rpm` package from [GitHub Releases](https://github.com/dbmodeling/dbml/releases), or install with the official script:
  ```bash
  curl -fsSL https://github.com/dbmodeling/dbml/releases/latest/download/db-modeling-install.sh | bash
  ```
* **Interactive Web CLI (`dbml`):** Start a local HTTP + WebSocket server with live reload and two-way persistence for `.dbml` and `.dbml.layout.json`:
  ```bash
  dbml path/to/schema.dbml
  # Options: --port 3000 --no-open --readonly --theme dark
  ```
* **VS Code Extension:** Open any `.dbml` file with the **DB Modeling** custom editor (`vscode-extension/`).

---

## Build from Source

To build the full workspace (Rust core, WebAssembly bridge, Web workbench, CLI, VS Code extension, and Desktop app) from source:

```bash
git clone https://github.com/dbmodeling/dbml.git
cd dbml
./scripts/sync-build.sh --release
```

Run the native desktop app or interactive web CLI directly with Cargo:

```bash
# Native Desktop App
cargo run -p app --release -- path/to/schema.dbml

# Interactive Web CLI
cargo run -p dbml-cli --release -- path/to/schema.dbml
```

<details>
<summary>System dependencies, VS Code packaging &amp; native installer scripts</summary>

#### Ubuntu / Debian System Dependencies

```bash
sudo apt update && sudo apt install -y \
  build-essential pkg-config cmake \
  libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libfontconfig1-dev libssl-dev
```

#### VS Code Extension (`vscode-extension/`)

```bash
cd vscode-extension
npm run compile
# Press F5 in VS Code or package a .vsix with:
npx @vscode/vsce package
```

#### Native Packaging Scripts (`scripts/`)

* **Debian / Ubuntu (`.deb`):** `./scripts/build-deb.sh`
* **Fedora / RHEL (`.rpm`):** `./scripts/build-fedora.sh`
* **macOS (`.dmg`):** `./scripts/build-macos.sh`
* **Windows (`.exe`):** `./scripts/build-windows.sh`

</details>

---

## Contributing

Contributions, bug reports, and feature requests are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for the full development setup, architecture guidelines, and CI checks.

1. Fork the repository and create a new branch (`git checkout -b feat/my-feature`).
2. Make your changes and ensure all tests pass with `cargo test --workspace`.
3. If you modified any Rust core crate, `dbml-wasm`, or `packages/canvas-web`, run `./scripts/sync-build.sh` to keep the Desktop App, Web CLI, and VS Code Extension in sync.
4. Open a Pull Request on [GitHub](https://github.com/dbmodeling/dbml/pulls) with a clear description of your changes.

---

## Code Signing & Privacy

Free code signing provided by [SignPath.io](https://about.signpath.io/), using a certificate by [SignPath Foundation](https://signpath.org/).

DB Modeling runs completely locally and does not collect telemetry or send your `.dbml` schemas to external servers. See [SECURITY.md](SECURITY.md) for our signing policy and vulnerability reporting instructions.

---

## License

Licensed under the **[MIT License](LICENSE)**.
