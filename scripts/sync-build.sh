#!/usr/bin/env bash
# ==============================================================================
# Unified Build & Synchronization Script (Desktop + CLI Web + VS Code)
#
# Ensures that any change in the Rust core (`dbml-analysis`, `dbml-edit`,
# `dbml-wasm`, `layout`, `schema`) or the shared web package (`packages/canvas-web`)
# is recompiled and propagated across all targets:
#   1. WebAssembly (`packages/canvas-web/src/wasm`)
#   2. CLI Web Bundle (`crates/cli/web/`)
#   3. VS Code Extension Bundle (`vscode-extension/dist/`)
#   4. Rust Verification (`cargo test` + `cargo check` for `cli` and `app`)
# ==============================================================================

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:---dev}"

echo "==> [1/5] Running Rust core tests..."
cargo test -p schema -p dbml-syntax -p dbml-analysis -p dbml-edit -p layout -p dbml-wasm

echo "==> [2/5] Building web assets (Wasm -> packages/canvas-web -> crates/cli/web/)..."
# O bundle versionado (crates/cli/web) é sempre release e sem wasm-opt, para ser reproduzível
# (o CI o reconstrói e falha se divergir). `--release` aqui só liga o wasm-opt, que reduz o .wasm
# mas depende da versão do binaryen; use-o para empacotar, não para commitar o bundle.
if [[ "$MODE" == "--release" ]]; then
  ./scripts/build-web-assets.sh --opt
else
  ./scripts/build-web-assets.sh
fi

echo "==> [3/5] Building VS Code Extension (vscode-extension/dist/)..."
rm -rf vscode-extension/dist # evita empacotar saídas antigas (ex.: webview.js)
cp -f LICENSE vscode-extension/LICENSE
cp -f crates/app/assets/sample.dbml vscode-extension/media/sample.dbml 2>/dev/null || true
(
  cd vscode-extension
  npm run compile
  npm run build
)

echo "==> [4/5] Checking that no embedded/packaged output is unreferenced..."
./scripts/check-web-assets.sh

echo "==> [5/5] Verifying Rust binaries (dbml-cli & app)..."
if [[ "$MODE" == "--release" ]]; then
  cargo build -p dbml-cli -p app --release
else
  cargo check -p dbml-cli -p app
fi

echo "✅ Synchronization complete! Desktop (app), CLI (dbml), and VS Code Extension are aligned."
