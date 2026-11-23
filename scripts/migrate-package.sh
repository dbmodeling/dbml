#!/usr/bin/env bash
# ==============================================================================
# Migração única do DB Modeling instalado por .deb/.rpm (v0.4.0 ou anterior).
#
# Essas versões trazem /usr/lib/db-modeling/update-explanation, que desliga o
# auto-update. Este script instala o pacote da última release por cima (upgrade
# no lugar, sem desinstalar; config e dados em ~/.config ficam intactos). O
# pacote novo não tem mais o arquivo, e dali em diante o app se atualiza sozinho.
#
# Uso:  ./scripts/migrate-package.sh [vX.Y.Z]     (padrão: última release)
# Variável opcional: REPO=dono/repositório
# ==============================================================================
set -euo pipefail

readonly PKG_NAME="db-modeling"
readonly DEFAULT_REPO="dbmodeling/dbml"
readonly ASSET_DEB="db-modeling-linux-amd64.deb"
readonly ASSET_RPM="db-modeling-linux-x86_64.rpm"
readonly SUMS_FILE="SHA256SUMS.txt"
readonly REPO="${REPO:-${DEFAULT_REPO}}"
readonly TAG_REGEX='^v[0-9]+\.[0-9]+\.[0-9]+$'

die() { echo "erro: $*" >&2; exit 1; }

if command -v dpkg >/dev/null 2>&1 && dpkg -s "${PKG_NAME}" >/dev/null 2>&1; then
  kind="deb"; asset="${ASSET_DEB}"
elif command -v rpm >/dev/null 2>&1 && rpm -q "${PKG_NAME}" >/dev/null 2>&1; then
  kind="rpm"; asset="${ASSET_RPM}"
else
  echo "${PKG_NAME} não está instalado por .deb/.rpm; nada a migrar."
  echo "Para instalar, use ./scripts/install.sh ou baixe o pacote em https://github.com/${REPO}/releases"
  exit 0
fi

tag="${1:-}"
if [[ -z "${tag}" ]]; then
  tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
fi
[[ "${tag}" =~ ${TAG_REGEX} ]] || die "tag inválida ou não encontrada: '${tag}'"

base="https://github.com/${REPO}/releases/download/${tag}"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

echo "==> Baixando ${asset} (${tag})..."
curl -fsSL -o "${work}/${asset}" "${base}/${asset}"
curl -fsSL -o "${work}/${SUMS_FILE}" "${base}/${SUMS_FILE}"

expected="$(awk -v f="${asset}" '$2 == f || $2 == "*" f { print $1 }' "${work}/${SUMS_FILE}" | head -n 1)"
[[ -n "${expected}" ]] || die "${asset} ausente em ${SUMS_FILE}"
actual="$(sha256sum "${work}/${asset}" | awk '{print $1}')"
[[ "${expected}" == "${actual}" ]] || die "SHA-256 não confere para ${asset}"

echo "==> Instalando por cima (pede senha de administrador)..."
if [[ "${kind}" == "deb" ]]; then
  sudo apt-get install -y "${work}/${asset}"
else
  sudo dnf install -y "${work}/${asset}"
fi

echo "Pronto. Reabra o ${PKG_NAME}: a partir daqui ele se atualiza sozinho."
