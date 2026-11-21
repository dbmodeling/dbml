#!/usr/bin/env bash
# ==============================================================================
# Rejeita dependências com faixas mutáveis (^ ou ~) nos package.json do projeto.
# Exceção: `engines` (ex.: `"vscode": "^1.85.0"`), que declara compatibilidade
# mínima da plataforma hospedeira.
#
# Uso: ./scripts/check-npm-pins.sh [arquivo.json ...] | --self-test
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

check_files() {
  node -e '
    const fs = require("fs");
    const sections = ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"];
    let bad = [];
    for (const file of process.argv.slice(1)) {
      const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
      for (const sec of sections) {
        const deps = pkg[sec];
        if (!deps || typeof deps !== "object") continue;
        for (const [name, ver] of Object.entries(deps)) {
          if (typeof ver === "string" && /^[\^~]/.test(ver.trim())) {
            bad.push(`${file}: ${sec}.${name} = ${ver}`);
          }
        }
      }
    }
    if (bad.length) {
      for (const line of bad) console.error("  " + line);
      process.exit(1);
    }
  ' "$@"
}

self_test() {
  local tmp rc=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  cat > "$tmp/ok.json" <<'EOF'
{
  "engines": { "vscode": "^1.85.0" },
  "dependencies": { "foo": "1.2.3" },
  "devDependencies": { "bar": "4.5.6" }
}
EOF
  check_files "$tmp/ok.json" || { echo "FAIL: falso positivo" >&2; rc=1; }
  cat > "$tmp/bad.json" <<'EOF'
{
  "dependencies": { "foo": "^1.2.3" },
  "devDependencies": { "bar": "~4.5.6" }
}
EOF
  if check_files "$tmp/bad.json" 2>/dev/null; then
    echo "FAIL: não detectou ^/~" >&2
    rc=1
  fi
  [[ $rc -eq 0 ]] && echo "ok - check-npm-pins self-test"
  return $rc
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

if [[ $# -gt 0 ]]; then
  files=("$@")
else
  files=(
    "$ROOT_DIR/packages/canvas-web/package.json"
    "$ROOT_DIR/vscode-extension/package.json"
  )
fi

if ! check_files "${files[@]}"; then
  echo "Dependências frontend com faixa mutável (^ ou ~); use versões exatas." >&2
  exit 1
fi
echo "✓ Todas as dependências frontend usam versões exatas."
