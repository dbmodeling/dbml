#!/usr/bin/env bash
# ==============================================================================
# Rejeita ações de workflow referenciadas por tag/branch mutável (@v4, @stable,
# @main...). Toda `uses:` externa precisa apontar para um commit completo
# (40 hex), com o nome da versão num comentário ao lado.
#
# Uso: ./scripts/check-workflow-pins.sh [diretório-de-workflows] | --self-test
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Imprime `arquivo:linha: texto` para cada `uses:` externa sem SHA completo.
find_unpinned() {
  local dir="$1" f
  for f in "$dir"/*.yml "$dir"/*.yaml; do
    [[ -f "$f" ]] || continue
    awk -v file="$f" '
      /^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*/ {
        ref = $0
        sub(/^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*/, "", ref)
        sub(/[[:space:]]*#.*$/, "", ref)          # comentário de versão
        gsub(/^["\x27]|["\x27]$/, "", ref)
        if (ref ~ /^\.\// || ref ~ /^docker:\/\//) next   # ação local / imagem
        if (ref !~ /@[0-9a-f]{40}$/) printf "%s:%d: %s\n", file, FNR, ref
      }' "$f"
  done
}

self_test() {
  local tmp rc=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  cat > "$tmp/ok.yml" <<'EOF'
steps:
  - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
  - uses: ./.local/action
  - uses: docker://alpine:3
EOF
  [[ -z "$(find_unpinned "$tmp")" ]] || { echo "FAIL: falso positivo" >&2; rc=1; }
  cat > "$tmp/bad.yml" <<'EOF'
steps:
  - uses: actions/checkout@v4
  - uses: dtolnay/rust-toolchain@stable
  - uses: actions/cache@11d5960a326750d5838078e36cf38b85af67726 # SHA curto
EOF
  [[ "$(find_unpinned "$tmp" | wc -l)" -eq 3 ]] || { echo "FAIL: não achou as 3 mutáveis" >&2; rc=1; }
  [[ $rc -eq 0 ]] && echo "ok - check-workflow-pins self-test"
  return $rc
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

DIR="${1:-$ROOT_DIR/.github/workflows}"
bad="$(find_unpinned "$DIR")"
if [[ -n "$bad" ]]; then
  echo "Ações referenciadas por tag/branch mutável (use o SHA completo do commit):" >&2
  echo "$bad" | sed 's/^/  /' >&2
  exit 1
fi
echo "✓ Todas as ações de workflow estão fixadas por SHA."
