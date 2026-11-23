#!/usr/bin/env bash
# Testes de scripts/{generate,sign,verify}-update-manifest.sh (sem rede, sem segredos reais).
# As assinaturas usam um par minisign DESCARTÁVEL gerado em diretório temporário pelo `rsign`;
# sem `rsign` no PATH, só os testes de geração rodam (e o aviso é explícito).
#
# Também confere que o manifesto gerado a partir do `release-dist` fictício é idêntico às
# fixtures consumidas pelo teste Rust (crates/updater/tests/manifest_fixture.rs).
# Uso: ./scripts/tests/update_manifest_test.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS="$HERE/.."
ROOT="$HERE/../.."
FIXTURE_DIR="$ROOT/crates/updater/tests/fixtures"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

readonly TAG="v0.4.0"
readonly VERSION="0.4.0"
readonly TEAM_ID="ABCDE12345"
readonly ASSET_WINDOWS="db-modeling-windows-setup.exe"
readonly ASSET_MACOS="db-modeling-macos-arm64.dmg"
readonly ASSET_LINUX="db-modeling-linux-x86_64.tar.gz"
readonly FIXTURE_SIGNED="latest.json"
readonly FIXTURE_UNSIGNED="latest-unsigned-macos.json"

FAILS=0
ok() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; FAILS=$((FAILS + 1)); }
expect_ok() { local name="$1"; shift; if "$@" >"$SANDBOX/out" 2>&1; then ok "$name"; else fail "$name"; sed 's/^/       /' "$SANDBOX/out"; fi; }
expect_err() { local name="$1"; shift; if "$@" >"$SANDBOX/out" 2>&1; then fail "$name (deveria falhar)"; else ok "$name"; fi; }

# --- ambiente fictício -----------------------------------------------------------------------
DIST="$SANDBOX/release-dist"
mkdir -p "$DIST" "$SANDBOX/tmp"
printf 'instalador fictício do windows\n' > "$DIST/$ASSET_WINDOWS"
printf 'imagem fictícia do macos\n' > "$DIST/$ASSET_MACOS"
printf 'tarball fictício do linux\n' > "$DIST/$ASSET_LINUX"
printf 'pacote que não entra no manifesto\n' > "$DIST/db-modeling-linux-amd64.deb"

FAKE_CARGO="$SANDBOX/Cargo.toml"
printf '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "%s"\nedition = "2021"\n\n[workspace.dependencies]\nfoo = { version = "9.9.9" }\n' "$VERSION" > "$FAKE_CARGO"

gen() { # gen <tag> [team_id] [out] -> usa CARGO_PKG_VERSION=$CARGO_PKG_VERSION_FOR_TEST se definida
    local tag="$1" team="${2:-}" out="${3:-$SANDBOX/out/latest.json}"
    env -u CARGO_PKG_VERSION RELEASE_TAG="$tag" RELEASE_DIST_DIR="$DIST" MANIFEST_OUTPUT="$out" \
        MACOS_TEAM_ID="$team" CARGO_MANIFEST_PATH="$FAKE_CARGO" TMPDIR="$SANDBOX/tmp" \
        bash "$SCRIPTS/generate-update-manifest.sh"
}

# --- geração: caminho feliz e fixtures --------------------------------------------------------
expect_ok  "gera manifesto estável com team_id"            gen "$TAG" "$TEAM_ID" "$SANDBOX/signed/latest.json"
expect_ok  "gera manifesto estável sem team_id"            gen "$TAG" "" "$SANDBOX/unsigned/latest.json"
expect_ok  "manifesto com team_id == fixture do teste Rust"   diff -u "$FIXTURE_DIR/$FIXTURE_SIGNED" "$SANDBOX/signed/latest.json"
expect_ok  "manifesto sem team_id == fixture do teste Rust"   diff -u "$FIXTURE_DIR/$FIXTURE_UNSIGNED" "$SANDBOX/unsigned/latest.json"
expect_ok  "macos sem build assinado não tem team_id"         jq -e '.platforms["macos-aarch64"] | has("team_id") | not' "$SANDBOX/unsigned/latest.json"
expect_ok  "macos assinado traz team_id"                      jq -e --arg t "$TEAM_ID" '.platforms["macos-aarch64"].team_id == $t' "$SANDBOX/signed/latest.json"
expect_ok  "schema 1 e versão da tag"                         jq -e --arg v "$VERSION" '.schema == 1 and .version == $v' "$SANDBOX/signed/latest.json"
expect_ok  "url no formato releases/download/<tag>/<ativo>"   jq -e --arg p "https://github.com/dbmodeling/dbml/releases/download/$TAG/" \
    '.platforms | to_entries | all(.value.url | startswith($p))' "$SANDBOX/signed/latest.json"
WIN_SHA="$(sha256sum "$DIST/$ASSET_WINDOWS" | awk '{print $1}')"
expect_ok  "sha256 é o do ativo real"                         jq -e --arg s "$WIN_SHA" '.platforms["windows-x86_64"].sha256 == $s' "$SANDBOX/signed/latest.json"
expect_ok  "geração não deixa temporários"                    test -z "$(find "$SANDBOX/signed" "$SANDBOX/unsigned" -name '.latest.json.*')"

# --- geração: falhas --------------------------------------------------------------------------
expect_err "tag != Cargo.toml (v0.4.1 vs 0.4.0) falha"        gen "v0.4.1" "" "$SANDBOX/div/latest.json"
expect_err "divergência não deixa manifesto"                  test -e "$SANDBOX/div/latest.json"
expect_err "CARGO_PKG_VERSION explícito divergente falha"     env CARGO_PKG_VERSION="0.3.9" RELEASE_TAG="$TAG" RELEASE_DIST_DIR="$DIST" MANIFEST_OUTPUT="$SANDBOX/div2.json" bash "$SCRIPTS/generate-update-manifest.sh"
expect_ok  "CARGO_PKG_VERSION explícito igual passa"          env CARGO_PKG_VERSION="$VERSION" RELEASE_TAG="$TAG" RELEASE_DIST_DIR="$DIST" MANIFEST_OUTPUT="$SANDBOX/explicit.json" bash "$SCRIPTS/generate-update-manifest.sh"
for asset in "$ASSET_WINDOWS" "$ASSET_MACOS" "$ASSET_LINUX"; do
    mv "$DIST/$asset" "$SANDBOX/$asset.bak"
    expect_err "ativo ausente ($asset) falha"                 gen "$TAG" "" "$SANDBOX/miss/latest.json"
    : > "$DIST/$asset"
    expect_err "ativo vazio ($asset) falha"                   gen "$TAG" "" "$SANDBOX/miss/latest.json"
    rm -f "$DIST/$asset"
    mv "$SANDBOX/$asset.bak" "$DIST/$asset"
done
expect_err "ativo ausente não deixa manifesto"                test -e "$SANDBOX/miss/latest.json"
for pre in "v0.4.0-rc.1" "v0.4.0-rc1" "v0.4.0-beta" "v0.4.0+build.5"; do
    expect_err "tag pré-release/build ($pre) não gera manifesto" gen "$pre" "" "$SANDBOX/pre/latest.json"
done
expect_err "pré-release não deixa manifesto"                  test -e "$SANDBOX/pre/latest.json"
for bad in "latest" "0.4.0" "v0.4" "v01.4.0" "" 'v0.4.0"; touch pwned; "'; do
    expect_err "tag inválida ('$bad') falha"                  gen "$bad" "" "$SANDBOX/bad/latest.json"
done
expect_err "tag maliciosa não executa nada"                   test -e "$SANDBOX/pwned"
expect_err "team_id inválido falha"                           gen "$TAG" 'abc"; touch pwned' "$SANDBOX/bad/latest.json"
expect_err "saída obrigatória: sem RELEASE_DIST_DIR falha"    env -u RELEASE_DIST_DIR RELEASE_TAG="$TAG" MANIFEST_OUTPUT="$SANDBOX/x.json" bash "$SCRIPTS/generate-update-manifest.sh"

# --- Cargo.toml real do repositório: versão do workspace é lida e confere ----------------------
REAL_VERSION="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f&&/^version/{gsub(/^version[ ]*=[ ]*"|".*$/,""); print; exit}' "$ROOT/Cargo.toml")"
expect_ok  "usa a versão real do workspace (v$REAL_VERSION)"  env -u CARGO_PKG_VERSION RELEASE_TAG="v$REAL_VERSION" RELEASE_DIST_DIR="$DIST" MANIFEST_OUTPUT="$SANDBOX/real.json" bash "$SCRIPTS/generate-update-manifest.sh"

# --- assinatura e verificação -------------------------------------------------------------------
if ! command -v rsign >/dev/null 2>&1; then
    echo "AVISO - rsign ausente: testes de assinatura/verificação PULADOS."
else
    KEYS="$SANDBOX/keys"
    mkdir -p "$KEYS"
    rsign generate -W -f -p "$KEYS/test.pub" -s "$KEYS/test.key" >/dev/null 2>&1
    rsign generate -W -f -p "$KEYS/other.pub" -s "$KEYS/other.key" >/dev/null 2>&1
    TEST_PUB="$(sed -n 2p "$KEYS/test.pub")"
    TEST_SECRET="$(cat "$KEYS/test.key")"
    OTHER_SECRET="$(cat "$KEYS/other.key")"

    SIGDIR="$SANDBOX/published"
    mkdir -p "$SIGDIR"
    cp "$DIST/$ASSET_WINDOWS" "$DIST/$ASSET_MACOS" "$DIST/$ASSET_LINUX" "$SIGDIR/"
    cp "$SANDBOX/signed/latest.json" "$SIGDIR/latest.json"

    sign() { env "$@" UPDATER_PUBLIC_KEY="$TEST_PUB" TMPDIR="$SANDBOX/tmp" bash "$SCRIPTS/sign-update-manifest.sh" "$SIGDIR/latest.json"; }
    verify() { env UPDATER_PUBLIC_KEY="${1:-$TEST_PUB}" bash "$SCRIPTS/verify-update-manifest.sh" "$SIGDIR"; }

    expect_err "segredo ausente falha"                        sign -u UPDATER_SIGNING_KEY
    expect_err "segredo vazio falha"                          sign UPDATER_SIGNING_KEY=""
    expect_err "sem segredo não cria .sig"                    test -e "$SIGDIR/latest.json.sig"
    expect_err "segredo de outro par falha (não valida na chave pública)" sign UPDATER_SIGNING_KEY="$OTHER_SECRET"
    expect_err "segredo errado não deixa .sig"                test -e "$SIGDIR/latest.json.sig"
    expect_err "lixo no segredo falha"                        sign UPDATER_SIGNING_KEY="isto não é uma chave"
    expect_ok  "assina com o par descartável"                 sign UPDATER_SIGNING_KEY="$TEST_SECRET"
    expect_ok  "assinatura é prehashed (algoritmo ED)"        test "$(sed -n 2p "$SIGDIR/latest.json.sig" | cut -c1-2)" = "RU"
    expect_ok  "chave temporária apagada ao fim"              test -z "$(find "$SANDBOX/tmp" -mindepth 1)"
    expect_ok  "verify: manifesto + ativos válidos"           verify
    expect_err "verify: chave pública de outro par falha"     verify "$(sed -n 2p "$KEYS/other.pub")"

    # A chave pública REAL (constants.rs) não aceita assinatura do par descartável.
    expect_err "chave primária embutida recusa o par descartável" env -u UPDATER_PUBLIC_KEY bash "$SCRIPTS/verify-update-manifest.sh" "$SIGDIR"
    expect_ok  "chave primária lida de constants.rs"          bash -c 'source "$1/scripts/lib/update-public-key.sh"; k="$(primary_public_key "$1")"; [[ "$k" =~ ^RW[A-Za-z0-9+/=]{54}$ ]]' _ "$ROOT"

    # Adulterações após a assinatura.
    cp "$SIGDIR/latest.json" "$SANDBOX/latest.json.orig"
    jq '.version = "9.9.9"' "$SANDBOX/latest.json.orig" > "$SIGDIR/latest.json"
    expect_err "verify: manifesto adulterado falha"           verify
    cp "$SANDBOX/latest.json.orig" "$SIGDIR/latest.json"
    printf 'adulterado\n' >> "$SIGDIR/$ASSET_LINUX"
    expect_err "verify: ativo com SHA-256 divergente falha"   verify
    printf 'tarball fictício do linux\n' > "$SIGDIR/$ASSET_LINUX"
    rm -f "$SIGDIR/$ASSET_MACOS"
    expect_err "verify: ativo ausente do rascunho falha"      verify
    cp "$DIST/$ASSET_MACOS" "$SIGDIR/$ASSET_MACOS"
    expect_ok  "verify: restaurado volta a passar"            verify

    # Assinatura legada (`sign -l`) não é gerável pelo `rsign sign`; a recusa dela é coberta por
    # `crates/updater` (verify.rs) e `rsign verify` sem `-l` espelha o cliente.
fi

echo
if [[ $FAILS -eq 0 ]]; then
    echo "update_manifest_test: tudo ok"
else
    echo "update_manifest_test: $FAILS falha(s)"
    exit 1
fi
