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

echo "==> [2/5] Building Wasm (dbml-wasm -> packages/canvas-web/src/wasm)..."
if command -v wasm-pack >/dev/null 2>&1; then
  if [[ "$MODE" == "--release" ]]; then
    wasm-pack build crates/dbml-wasm --target web --release --out-dir ../../packages/canvas-web/src/wasm
  else
    wasm-pack build crates/dbml-wasm --target web --dev --out-dir ../../packages/canvas-web/src/wasm
  fi
elif command -v wasm-bindgen >/dev/null 2>&1; then
  if [[ "$MODE" == "--release" ]]; then
    cargo build -p dbml-wasm --target wasm32-unknown-unknown --release
    wasm-bindgen --target web --out-dir packages/canvas-web/src/wasm target/wasm32-unknown-unknown/release/dbml_wasm.wasm
  else
    cargo build -p dbml-wasm --target wasm32-unknown-unknown
    wasm-bindgen --target web --out-dir packages/canvas-web/src/wasm target/wasm32-unknown-unknown/debug/dbml_wasm.wasm
  fi
else
  echo "Error: neither 'wasm-pack' nor 'wasm-bindgen' was found in PATH." >&2
  exit 1
fi
rm -f packages/canvas-web/src/wasm/.gitignore

echo "==> [3/5] Building packages/canvas-web (Lib + Web App) and syncing crates/cli/web/..."
(
  cd packages/canvas-web
  npm run build
  npm run build:app
)
rm -rf crates/cli/web/assets
cp -r packages/canvas-web/dist-app/* crates/cli/web/

echo "==> [4/5] Building VS Code Extension (vscode-extension/dist/)..."
cp -f LICENSE vscode-extension/LICENSE
cp -f crates/app/assets/sample.dbml vscode-extension/media/sample.dbml 2>/dev/null || true
(
  cd vscode-extension
  npm run compile
)

echo "==> [5/5] Verifying Rust binaries (dbml-cli & app)..."
if [[ "$MODE" == "--release" ]]; then
  cargo build -p dbml-cli -p app --release
else
  cargo check -p dbml-cli -p app
fi

echo "✅ Synchronization complete! Desktop (app), CLI (dbml), and VS Code Extension are aligned."
