#!/usr/bin/env bash
# Testes da verificação de integridade de scripts/install.sh (sem rede nem sudo).
# Uso: ./scripts/tests/install_test.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES="$(mktemp -d)"
trap 'rm -rf "$FIXTURES"' EXIT

DBML_INSTALL_LIB=1 source "$HERE/../install.sh"
set +e # as funções sob teste retornam != 0 de propósito

FAILS=0
ok() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; FAILS=$((FAILS + 1)); }
expect_ok() { local name="$1"; shift; if "$@" 2>/dev/null; then ok "$name"; else fail "$name"; fi; }
expect_err() { local name="$1"; shift; if "$@" 2>/dev/null; then fail "$name"; else ok "$name"; fi; }

# --- verify_sha256 -----------------------------------------------------------
printf 'pacote-bom' > "$FIXTURES/pkg.deb"
GOOD="$(sha256_of "$FIXTURES/pkg.deb")"
printf '%s  pkg.deb\n%s  outro.rpm\n' "$GOOD" "0000" > "$FIXTURES/sums.txt"
printf '%s *pkg.deb\n' "$GOOD" > "$FIXTURES/sums-binary.txt"
printf '%s  pkg.deb\n' "$(printf x | sha256sum | awk '{print $1}')" > "$FIXTURES/sums-bad.txt"
printf '%s  other.deb\n' "$GOOD" > "$FIXTURES/sums-missing.txt"

expect_ok  "hash correto passa"                       verify_sha256 "$FIXTURES/pkg.deb" pkg.deb "$FIXTURES/sums.txt"
expect_ok  "formato binário (*nome) passa"            verify_sha256 "$FIXTURES/pkg.deb" pkg.deb "$FIXTURES/sums-binary.txt"
expect_err "hash divergente aborta"                   verify_sha256 "$FIXTURES/pkg.deb" pkg.deb "$FIXTURES/sums-bad.txt"
expect_err "arquivo ausente do SHA256SUMS aborta"     verify_sha256 "$FIXTURES/pkg.deb" pkg.deb "$FIXTURES/sums-missing.txt"
expect_err "nome parcial não casa (outro.rpm != pkg)" verify_sha256 "$FIXTURES/pkg.deb" pk "$FIXTURES/sums.txt"

# --- download_verified com curl simulado --------------------------------------
REL="$FIXTURES/release"
mkdir -p "$REL"
CALLS="$FIXTURES/calls"
: > "$CALLS"
# Simula `curl -fsSL URL -o DEST` copiando do diretório da "release".
curl() {
    local url="" dest="" prev=""
    for a in "$@"; do
        [ "$prev" = "-o" ] && dest="$a"
        case "$a" in http*) url="$a" ;; esac
        prev="$a"
    done
    echo "$url" >> "$CALLS"
    local src="$REL/$(basename "$url")"
    [ -f "$src" ] || return 22
    cp "$src" "$dest"
}

cp "$FIXTURES/pkg.deb" "$REL/pkg.deb"
cp "$FIXTURES/sums.txt" "$REL/$SUMS_FILE"
expect_ok "download verificado grava o arquivo" download_verified v9.9.9 pkg.deb "$FIXTURES/out.deb"
[ -f "$FIXTURES/out.deb" ] && ok "arquivo presente após sucesso" || fail "arquivo presente após sucesso"
grep -q '/releases/download/v9.9.9/pkg.deb' "$CALLS" && ok "baixa pela tag resolvida, não por latest" || fail "baixa pela tag resolvida"
grep -q 'releases/latest' "$CALLS" && fail "não usa releases/latest" || ok "não usa releases/latest"

printf 'adulterado' > "$REL/pkg.deb"
rm -f "$FIXTURES/out2.deb"
expect_err "pacote adulterado aborta" download_verified v9.9.9 pkg.deb "$FIXTURES/out2.deb"
[ -e "$FIXTURES/out2.deb" ] && fail "pacote adulterado é apagado" || ok "pacote adulterado é apagado"

rm -f "$REL/$SUMS_FILE"
expect_err "release sem SHA256SUMS aborta" download_verified v9.9.9 pkg.deb "$FIXTURES/out3.deb"
[ -e "$FIXTURES/out3.deb" ] && fail "sem SHA256SUMS não deixa arquivo" || ok "sem SHA256SUMS não deixa arquivo"

# --- resolve_tag -------------------------------------------------------------
DBML_VERSION=v1.2.3 resolve_tag | grep -qx 'v1.2.3' && ok "DBML_VERSION fixa a tag" || fail "DBML_VERSION fixa a tag"
curl() { echo '{"tag_name": "v4.5.6", "name": "x"}'; }
unset DBML_VERSION
[ "$(resolve_tag)" = "v4.5.6" ] && ok "tag vem de releases/latest da API" || fail "tag vem da API"
curl() { echo '{"message": "Not Found"}'; }
expect_err "resposta sem tag_name aborta" resolve_tag

echo
if [ "$FAILS" -ne 0 ]; then
    echo "$FAILS teste(s) falharam."
    exit 1
fi
echo "Todos os testes passaram."
