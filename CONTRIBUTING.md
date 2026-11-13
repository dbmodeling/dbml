# Contributing to DB Modeling

Thank you for considering contributing to **DB Modeling**! Whether you are fixing a bug, adding a feature, or improving documentation, this guide will help you get started.

---

## Project Structure

DB Modeling shares a single Rust core across three targets:

| Component | Path | Description |
| :--- | :--- | :--- |
| **Core Crates** | `crates/schema`, `crates/dbml-syntax`, `crates/dbml-analysis`, `crates/dbml-edit`, `crates/layout` | Parser (Rowan CST), linter, autocomplete, minimal textual edits, and Sugiyama auto-layout |
| **Desktop App** | `crates/app` | Native GPUI desktop application |
| **WebAssembly Bridge** | `crates/dbml-wasm` | Wasm bindings exposing the Rust core to browser and VS Code |
| **Shared Web Workbench** | `packages/canvas-web` | TypeScript/HTML5 Canvas UI shared by the Web CLI and VS Code Extension |
| **Interactive Web CLI** | `crates/cli` | Local HTTP + WebSocket server (`dbml` binary) |
| **VS Code Extension** | `vscode-extension/` | Custom Editor and Sidebar integration |

---

## Development Setup

### Prerequisites

* **Rust** (stable toolchain with `rustfmt` and `clippy`)
* **Node.js & npm** (for `packages/canvas-web` and `vscode-extension/`)
* **wasm-pack** or `wasm-bindgen-cli` + `wasm32-unknown-unknown` target (for WebAssembly builds)
* **Linux system libraries** (Ubuntu / Debian):
  ```bash
  sudo apt update && sudo apt install -y \
    build-essential pkg-config cmake \
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
    libvulkan-dev libfontconfig1-dev libssl-dev \
    libfreetype6-dev libasound2-dev libx11-xcb-dev libxcb-xkb-dev
  ```

### Getting Started

1. Fork the repository and clone your fork:
   ```bash
   git clone https://github.com/<your-username>/dbml.git
   cd dbml
   ```
2. Create a feature or fix branch:
   ```bash
   git checkout -b feat/my-feature
   ```

---

## Parity & Synchronization

Whenever you change a Rust core crate, `crates/dbml-wasm`, or `packages/canvas-web`, run the unified sync script before committing so the embedded Web CLI bundle (`crates/cli/web/`) and VS Code extension (`vscode-extension/dist/`) stay up to date:

```bash
./scripts/sync-build.sh
```

If you change UI behavior, keyboard shortcuts, or visual styling in `crates/app`, please port the equivalent behavior to `packages/canvas-web` (and vice versa) to maintain 1:1 parity across all three targets.

---

## Architectural Guidelines

* **DBML is the single source of truth:** Never store visual metadata (positions, colors) inside `.dbml` comments or custom syntax. Visual state belongs in `<file>.dbml.layout.json` with deterministic key ordering.
* **Minimal textual patches:** Visual edits on the canvas or inspector must generate minimal edits via `dbml-edit` that preserve user comments and formatting.
* **Core free of UI dependencies:** `schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`, and `dbml-wasm` must not depend on `gpui`, `tokio`, or OS-specific APIs.
* **Lossless error recovery:** The parser (`dbml-syntax`) must never panic on incomplete or invalid input.

---

## Checks Before Opening a Pull Request

CI runs the following checks on every pull request (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)):

```bash
# 1. Check formatting
cargo fmt --all -- --check

# 2. Run Clippy with warnings treated as errors
cargo clippy --workspace --all-targets -- -D warnings

# 3. Run the full workspace test suite
cargo test --workspace
```

If you updated Wasm or web assets, also run `./scripts/sync-build.sh`.

---

## Reporting Bugs & Suggesting Features

* **Bugs & Feature Requests:** Open an issue on [GitHub Issues](https://github.com/dbmodeling/dbml/issues) with reproduction steps or a clear use case.
* **Security Vulnerabilities:** Please report privately via [GitHub Security Advisories](https://github.com/dbmodeling/dbml/security/advisories/new) as described in [SECURITY.md](SECURITY.md).
