#!/usr/bin/env bash
# ==============================================================================
# Falha se houver saída de build empacotada que ninguém referencia.
#
#  - crates/cli/web/ (embutido na CLI): todo arquivo, exceto index.html, precisa
#    ser citado pelo nome em outro arquivo do mesmo diretório (index.html -> JS ->
#    .wasm/chunks). Um `app.js` solto, por exemplo, vai para o binário sem uso.
#  - vscode-extension/dist/ (se existir): todo arquivo, exceto `*.map`, precisa
#    ser citado em vscode-extension/src ou ser o `main` do package.json.
#
# Uso: ./scripts/check-web-assets.sh [--self-test]
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# unreferenced <dir> <arquivo-de-entrada-ou-vazio> <caminho-de-busca>...
# Imprime (um por linha) os arquivos de <dir> cujo nome base não aparece em nenhum dos
# caminhos de busca, exceto o próprio arquivo e a entrada.
unreferenced() {
  local dir="$1" entry="$2"
  shift 2
  local file base
  while IFS= read -r -d '' file; do
    base="$(basename "$file")"
    [[ "$base" == "$entry" || "$base" == *.map ]] && continue
    if ! grep -rqsF --exclude="$base" -- "$base" "$@"; then
      echo "${file#"$dir"/}"
    fi
  done < <(find "$dir" -type f -print0 | sort -z)
}

check_cli_web() {
  local dir="$ROOT_DIR/crates/cli/web"
  [[ -d "$dir" ]] || { echo "Aviso: $dir não existe; nada a verificar." >&2; return 0; }
  local bad
  bad="$(unreferenced "$dir" index.html "$dir")"
  if [[ -n "$bad" ]]; then
    echo "Arquivos em crates/cli/web/ que nenhum outro arquivo referencia:" >&2
    echo "$bad" | sed 's/^/  - /' >&2
    echo "Remova-os (a CLI os embute sem uso) ou rode ./scripts/build-web-assets.sh." >&2
    return 1
  fi
}

check_vscode_dist() {
  local dir="$ROOT_DIR/vscode-extension/dist"
  [[ -d "$dir" ]] || return 0
  local main bad
  main="$(sed -n 's/.*"main"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT_DIR/vscode-extension/package.json" | head -n 1)"
  bad="$(unreferenced "$dir" "$(basename "${main:-extension.cjs}")" "$ROOT_DIR/vscode-extension/src")"
  if [[ -n "$bad" ]]; then
    echo "Arquivos em vscode-extension/dist/ que o código da extensão não referencia:" >&2
    echo "$bad" | sed 's/^/  - /' >&2
    echo "Apague dist/ e reconstrua (./scripts/sync-build.sh)." >&2
    return 1
  fi
}

self_test() {
  local tmp rc=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  mkdir -p "$tmp/assets"
  echo '<script src="/assets/index-1.js"></script>' > "$tmp/index.html"
  echo 'new URL("dbml_wasm_bg-2.wasm")' > "$tmp/assets/index-1.js"
  echo 'wasm' > "$tmp/assets/dbml_wasm_bg-2.wasm"
  [[ -z "$(unreferenced "$tmp" index.html "$tmp")" ]] || { echo "FAIL: falso positivo" >&2; rc=1; }
  echo 'orfao' > "$tmp/app.js"
  [[ "$(unreferenced "$tmp" index.html "$tmp")" == "app.js" ]] || { echo "FAIL: não achou o órfão" >&2; rc=1; }
  [[ $rc -eq 0 ]] && echo "ok - check-web-assets self-test"
  return $rc
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

status=0
check_cli_web || status=1
check_vscode_dist || status=1
[[ $status -eq 0 ]] && echo "✓ Nenhuma saída de build sem referência."
exit $status
