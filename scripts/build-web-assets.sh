#!/usr/bin/env bash
# ==============================================================================
# Gera os ativos web compartilhados: Wasm (dbml-wasm) -> packages/canvas-web ->
# crates/cli/web/ (embutido no binário da CLI via rust-embed).
#
# Usado por scripts/sync-build.sh, pelo job `web-bundle` do CI (que falha se o
# bundle versionado divergir do reconstruído) e pelo workflow de release (que
# reconstrói a partir da tag em vez de confiar no bundle commitado).
#
# Uso: ./scripts/build-web-assets.sh [--dev] [--opt]
#   --dev  Wasm em perfil debug (mais rápido, maior; NÃO use para o bundle versionado)
#   --opt  roda `wasm-opt -Oz` no .wasm quando o binário existir no PATH
#          (reduz o módulo; o resultado depende da versão do binaryen, por isso
#          o bundle versionado e verificado pelo CI é gerado sem esta flag)
#
# Reprodutibilidade: caminhos absolutos são remapeados para que o mesmo commit
# gere os mesmos bytes em qualquer máquina.
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROFILE="release"
OPT=0
for arg in "$@"; do
  case "$arg" in
    --dev) PROFILE="dev" ;;
    --opt) OPT=1 ;;
    *) echo "Uso: $0 [--dev] [--opt]" >&2; exit 2 ;;
  esac
done

WASM_DIR="packages/canvas-web/src/wasm"
LOCKED_BINDGEN="$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[version = "]/, ""); print; exit}' Cargo.lock)"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "Erro: 'wasm-bindgen' não encontrado no PATH (instale wasm-bindgen-cli ${LOCKED_BINDGEN})." >&2
  exit 1
fi
CLI_BINDGEN="$(wasm-bindgen --version | awk '{print $2}')"
if [[ "$CLI_BINDGEN" != "$LOCKED_BINDGEN" ]]; then
  echo "Erro: wasm-bindgen CLI ${CLI_BINDGEN} difere da biblioteca travada no Cargo.lock (${LOCKED_BINDGEN})." >&2
  exit 1
fi

SYSROOT="$(rustc --print sysroot)"
RUSTC_HASH="$(rustc -vV | awk '/^commit-hash:/ {print $2}')"
REMAP="--remap-path-prefix=${ROOT_DIR}=. --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${SYSROOT}/lib/rustlib/src/rust=/rustc/${RUSTC_HASH} --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup"
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }${REMAP}"

echo "==> Compilando dbml-wasm (${PROFILE})..."
if [[ "$PROFILE" == "release" ]]; then
  cargo build --locked -p dbml-wasm --target wasm32-unknown-unknown --release
  WASM_IN="target/wasm32-unknown-unknown/release/dbml_wasm.wasm"
else
  cargo build --locked -p dbml-wasm --target wasm32-unknown-unknown
  WASM_IN="target/wasm32-unknown-unknown/debug/dbml_wasm.wasm"
fi

echo "==> wasm-bindgen -> ${WASM_DIR}"
mkdir -p "$WASM_DIR"
wasm-bindgen --target web --out-dir "$WASM_DIR" "$WASM_IN"
rm -f "$WASM_DIR/.gitignore"

WASM_OUT="$WASM_DIR/dbml_wasm_bg.wasm"
BEFORE="$(wc -c < "$WASM_OUT")"
if [[ "$OPT" == 1 ]]; then
  if command -v wasm-opt >/dev/null 2>&1; then
    echo "==> wasm-opt -Oz"
    wasm-opt -Oz \
      --enable-bulk-memory --enable-sign-ext --enable-nontrapping-float-to-int \
      --enable-mutable-globals --enable-reference-types --enable-multivalue \
      -o "$WASM_OUT.opt" "$WASM_OUT"
    mv "$WASM_OUT.opt" "$WASM_OUT"
  else
    echo "Aviso: --opt pedido mas 'wasm-opt' (binaryen) não está no PATH; mantendo o .wasm sem otimizar." >&2
  fi
fi
echo "    dbml_wasm_bg.wasm: ${BEFORE} -> $(wc -c < "$WASM_OUT") bytes"

echo "==> Compilando packages/canvas-web (lib + app)..."
(
  cd packages/canvas-web
  if [[ ! -d node_modules ]]; then npm ci; fi
  npm run build
  npm run build:app
)

echo "==> Sincronizando crates/cli/web/ (limpa saídas antigas)..."
find crates/cli/web -mindepth 1 -delete
cp -r packages/canvas-web/dist-app/* crates/cli/web/
echo "✓ Ativos web gerados."
