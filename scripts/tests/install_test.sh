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

# --- instalação de usuário (tarball em ~/.local, sem sudo) -----------------------
ROOT="$(cd "$HERE/../.." && pwd)"
LX="$FIXTURES/linux"
mkdir -p "$LX/home" "$LX/fakebin" "$LX/dist"
# sudo/apt/dnf/pkexec de mentira: qualquer chamada vira falha registrada.
for tool in sudo apt-get dnf pkexec; do
    printf '#!/bin/sh\necho "%s $*" >> "%s/forbidden.log"\nexit 1\n' "$tool" "$LX" > "$LX/fakebin/$tool"
    chmod +x "$LX/fakebin/$tool"
done
make_tarball() { # <conteúdo do executável> <saída>
    printf '#!/bin/sh\necho %s\n' "$1" > "$LX/stub"
    chmod +x "$LX/stub"
    DBML_APP_BIN="$LX/stub" DIST_DIR="$2" "$ROOT/scripts/build-linux-tarball.sh" >/dev/null
}
run_install() { # roda o script de verdade com HOME falso e sem sudo real
    env -i HOME="$LX/home" PATH="$LX/fakebin:/usr/bin:/bin" "$ROOT/scripts/install.sh" "$@" >"$LX/out.log" 2>&1
}
APP="$LX/home/.local/db-modeling.app"
DATA="$LX/home/.local/share"

make_tarball v1 "$LX/dist"
expect_ok  "install.sh padrão instala o tarball local" run_install "$LX/dist/$LINUX_TARBALL"
[ -x "$APP/bin/db-modeling" ] && ok "binário em ~/.local/db-modeling.app/bin" || fail "binário em ~/.local/db-modeling.app/bin"
[ "$(readlink "$LX/home/.local/bin/db-modeling")" = "$APP/bin/db-modeling" ] && ok "symlink em ~/.local/bin" || fail "symlink em ~/.local/bin"
grep -qx "Exec=\"$APP/bin/db-modeling\" %F" "$DATA/applications/db-modeling.desktop" && ok ".desktop com Exec absoluto" || fail ".desktop com Exec absoluto"
grep -qx "Icon=$APP/share/icons/hicolor/256x256/apps/db-modeling.png" "$DATA/applications/db-modeling.desktop" && ok ".desktop com Icon absoluto" || fail ".desktop com Icon absoluto"
[ -f "$DATA/icons/hicolor/scalable/apps/db-modeling.svg" ] && [ -f "$DATA/icons/hicolor/256x256/apps/db-modeling.png" ] && ok "ícones do usuário" || fail "ícones do usuário"
[ -f "$DATA/mime/packages/db-modeling.xml" ] && ok "tipo MIME do usuário" || fail "tipo MIME do usuário"
[ -e "$LX/forbidden.log" ] && fail "nunca chama sudo/apt/dnf/pkexec" || ok "nunca chama sudo/apt/dnf/pkexec"

# reinstalar troca a pasta inteira e não mexe no resto de ~/.local
touch "$APP/arquivo-velho" "$LX/home/.local/bin/meu-script"
make_tarball v2 "$LX/dist"
expect_ok  "reinstalar atualiza" run_install "$LX/dist/$LINUX_TARBALL"
grep -q v2 "$APP/bin/db-modeling" && ok "versão nova no lugar" || fail "versão nova no lugar"
[ -e "$APP/arquivo-velho" ] && fail "reinstalar remove o que a versão nova não tem" || ok "reinstalar remove o que a versão nova não tem"
[ -e "$LX/home/.local/bin/meu-script" ] && ok "arquivos do usuário em ~/.local intactos" || fail "arquivos do usuário em ~/.local intactos"

# um arquivo comum em ~/.local/bin não é sobrescrito pelo symlink
rm "$LX/home/.local/bin/db-modeling"
printf 'meu' > "$LX/home/.local/bin/db-modeling"
expect_ok  "binário do usuário em ~/.local/bin é preservado" run_install "$LX/dist/$LINUX_TARBALL"
[ "$(cat "$LX/home/.local/bin/db-modeling")" = "meu" ] && ok "arquivo comum não vira symlink" || fail "arquivo comum não vira symlink"
rm "$LX/home/.local/bin/db-modeling"

# tarball inválido não deixa nada para trás
printf 'lixo' > "$LX/ruim.tar.gz"
expect_err "tarball inválido aborta" run_install "$LX/ruim.tar.gz"
grep -q v2 "$APP/bin/db-modeling" && ok "instalação anterior intacta após falha" || fail "instalação anterior intacta após falha"

expect_ok  "--uninstall remove a instalação de usuário sem sudo" run_install --uninstall
[ -e "$APP" ] || [ -e "$DATA/applications/db-modeling.desktop" ] && fail "--uninstall remove app e atalho" || ok "--uninstall remove app e atalho"
[ -e "$LX/home/.local/bin/meu-script" ] && ok "--uninstall preserva arquivos do usuário" || fail "--uninstall preserva arquivos do usuário"
[ -e "$LX/forbidden.log" ] && fail "--uninstall de usuário nunca chama sudo" || ok "--uninstall de usuário nunca chama sudo"
expect_err "opção desconhecida aborta" run_install --nao-existe

echo
if [ "$FAILS" -ne 0 ]; then
    echo "$FAILS teste(s) falharam."
    exit 1
fi
echo "Todos os testes passaram."
