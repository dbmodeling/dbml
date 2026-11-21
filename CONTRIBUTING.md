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

How CI works ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)), in five lines:

1. **PR into `develop`:** a cheap preflight (branch name, changed paths, `cargo fmt`, scripts), then the full Linux/Windows/macOS + web suite for the areas the PR touches. This is the gate.
2. **Merge into `develop`:** the suite runs once more on the squashed commit. It catches the rare break when `develop` moved after the PR ran, and it is the **only run that saves the build cache**.
3. **PR `develop → main`:** nothing is rebuilt; the `gate` job requires the `develop` run of that exact commit to be green.
4. **PR `release/*`, `hotfix/*`, `dependabot/*` → `main`, and Actions → CI → "Run workflow" on any branch:** full suite.
5. **The only required check is `CI ok`**; a job skipped on purpose (path filter, draft PR) counts as ok. Fedora runs weekly on `develop` ([`fedora.yml`](.github/workflows/fedora.yml)).

Pushes to a work branch without a PR run nothing, and draft PRs run only the preflight.

Why only `develop` saves the cache: GitHub scopes a cache to the ref that created it. A PR's cache
lives on its merge ref and no other run can restore it; it only fills the 10 GB repository quota
and evicts the `develop` cache. Every PR into `develop` restores the `develop` cache (~3.5 min on
Windows/macOS); without it, ~400 crates build from scratch (CI builds them unoptimised to keep
that under ~10 min). A slow PR run means the
`develop` cache is missing or being rebuilt (e.g. right after a toolchain change).

Why no merge queue: queue runs live on `gh-readonly-queue/develop/*` refs, which can restore caches
only from their own ref and from `main`, not from `develop`, so every queued merge would build cold.

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
