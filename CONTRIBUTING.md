# Contributing to DB Modeling

Thank you for considering contributing to **DB Modeling**! Whether you are fixing a bug, adding a feature, or improving documentation, this guide will help you get started.

---

## Project Structure

DB Modeling shares a single Rust core across three targets:

| Component | Path | Description |
| :--- | :--- | :--- |
| **Core Crates** | `crates/core/schema`, `crates/core/dbml-syntax`, `crates/core/dbml-analysis`, `crates/core/dbml-edit`, `crates/core/layout` | Parser (Rowan CST), linter, autocomplete, minimal textual edits, and Sugiyama auto-layout |
| **Desktop App** | `crates/app` | Native GPUI desktop application |
| **WebAssembly Bridge** | `crates/dbml-wasm` | Wasm bindings exposing the Rust core to browser and VS Code |
| **Shared Web Workbench** | `packages/canvas-web` | TypeScript/HTML/SVG workbench shared by the Web CLI and VS Code Extension |
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
2. Create your branch from `develop`, following the [branch naming rules](#branching-model):
   ```bash
   git switch develop && git pull
   git switch -c feat/my-feature
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

## Branching Model

```
develop ──▶ feat|bug|improve|chore|ci/<desc> ──PR (squash)──────▶ develop
develop ──▶ release/<version> ────────────────PR (merge commit)─▶ main ──tag v*──▶ release
develop ─────────────────────────────────────PR (merge commit)─▶ main
main    ──▶ hotfix/<desc> ───PR (merge commit)─▶ main ──PR main→develop (squash)─▶ develop
```

| Pull request into | Accepted source branches |
| :--- | :--- |
| `develop` | `feat/…`, `bug/…`, `improve/…`, `chore/…`, `ci/…` (lowercase kebab-case), `dependabot/…`, `main` (hotfix back-merge) |
| `main` | `develop`, `release/vX.Y.Z`, `hotfix/…`, `dependabot/…` (security updates) |

* `develop` and `main` only change through pull requests. The single required check is **`CI ok`**.
* PRs into `develop` are **squash-merged**; PRs into `main` use a **merge commit**.
* After a hotfix lands on `main`, open the `main → develop` back-merge right away.
* Only organization owners can force-push or delete `develop` and `main` (for exceptional
  rebase fixes).

## Continuous Integration

Each commit runs the full suite once before other work builds on it, and the gate is the pull request:

| Event | What runs |
| :--- | :--- |
| Push to a work branch (no PR) | Nothing. Run the checks below locally. |
| Draft PR | Light checks only (branch name, `cargo fmt`). |
| PR into `develop` | Preflight first (branch name, paths, `cargo fmt`, script checks), then clippy + tests on Ubuntu, Windows and macOS and the Web job, only for the areas changed. |
| Merge (push) into `develop` | Full suite, unfiltered. **This is the only run that saves the build cache.** |
| PR `develop → main` | No re-run: the `gate` job requires the push run of the same commit on `develop` to have passed. |
| PR `release/*`, `hotfix/*`, `dependabot/*` → `main` | Full suite. |
| Every Monday (and on demand) | `Fedora (weekly)`: the test suite on `develop` inside a Fedora container. It is not part of per-commit CI because it shares the Ubuntu rustc and target; only the system libraries differ. |

Why PRs never save cache: GitHub scopes a PR's cache to its merge ref, so no other run can restore
it. It only fills the 10 GB repository quota and evicts the `develop` cache. Without the cache,
Windows/macOS build GPUI from scratch (~19 min instead of ~3 min).

Dependabot opens at most one grouped PR per ecosystem every Monday (06:00 America/Manaus) against
`develop`, without auto-rebase. Comment `@dependabot rebase` to refresh one.

## Checks Before Opening a Pull Request

CI runs the following checks on pull requests (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)):

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

## Pull Requests and Releases

GitHub loads the [pull request template](.github/pull_request_template.md) when opening a PR. For UI changes, attach before and after screenshots from each affected target and name the target in the caption.

Use the [release template](.github/RELEASE_TEMPLATE.md) to draft release notes and verify the assets published by the [release workflow](.github/workflows/release.yml). See the [workbench guide](docs/05-ux.md), [architecture](docs/03-arquitetura.md), and [visual identity](docs/identidade-visual.md) when updating documentation.

## Reporting Bugs & Suggesting Features

* **Bugs & Feature Requests:** Open    an issue on [GitHub Issues](https://github.com/dbmodeling/dbml/issues) with reproduction steps or a clear use case.
* **Security Vulnerabilities:** Please report privately via [GitHub Security Advisories](https://github.com/dbmodeling/dbml/security/advisories/new) as described in [SECURITY.md](SECURITY.md).
