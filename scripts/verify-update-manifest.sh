#!/usr/bin/env bash
# ==============================================================================
# Verifica um manifesto de auto-update já publicado (ex.: baixado do RASCUNHO da
# Release) antes de torná-lo público:
#   1. `latest.json.sig` valida `latest.json` com a chave pública primária embutida
#      no cliente (`rsign verify` sem `-l`: assinatura legada é recusada);
#   2. toda plataforma do manifesto aponta para um ativo da própria tag
#      (`https://github.com/<repo>/releases/download/v<versão>/<ativo>`);
#   3. cada ativo referenciado existe no diretório e o SHA-256 confere.
#
# Uso: ./scripts/verify-update-manifest.sh <diretório-com-latest.json-sig-e-ativos>
# Entradas opcionais por ambiente: RELEASE_REPO (padrão dbmodeling/dbml).
# ==============================================================================
set -euo pipefail

readonly DEFAULT_RELEASE_REPO="dbmodeling/dbml"
readonly MANIFEST_FILE_NAME="latest.json"
readonly SIGNATURE_FILE_NAME="latest.json.sig"
# windows, macos, linux (tarball), linux-deb e linux-rpm
readonly EXPECTED_PLATFORM_COUNT=5

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/update-public-key.sh
source "${ROOT_DIR}/scripts/lib/update-public-key.sh"

die() {
  echo "::error::verify-update-manifest: $*" >&2
  exit 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

main() {
  [[ $# -eq 1 ]] || die "uso: $0 <diretório>"
  local dir="$1" repo="${RELEASE_REPO:-$DEFAULT_RELEASE_REPO}"
  local manifest="${dir}/${MANIFEST_FILE_NAME}" signature="${dir}/${SIGNATURE_FILE_NAME}"
  command -v jq >/dev/null 2>&1 || die "jq é obrigatório."
  command -v rsign >/dev/null 2>&1 || die "rsign não encontrado (instale rsign2 0.6.7)."
  [[ -f "$manifest" ]] || die "${MANIFEST_FILE_NAME} ausente em ${dir}"
  [[ -f "$signature" ]] || die "${SIGNATURE_FILE_NAME} ausente em ${dir}"

  local public_key
  public_key="$(primary_public_key "$ROOT_DIR")"
  rsign verify -q -P "$public_key" -x "$signature" "$manifest" \
    || die "assinatura de ${MANIFEST_FILE_NAME} inválida para a chave pública primária."

  local version prefix count
  version="$(jq -er '.version' "$manifest")" || die "manifesto sem 'version'."
  prefix="https://github.com/${repo}/releases/download/v${version}/"
  count="$(jq -er '.platforms | length' "$manifest")" || die "manifesto sem 'platforms'."
  [[ "$count" -ge "$EXPECTED_PLATFORM_COUNT" ]] \
    || die "manifesto com ${count} plataformas (esperado >= ${EXPECTED_PLATFORM_COUNT})."

  local platform url sha name actual
  while IFS=$'\t' read -r platform url sha; do
    [[ "$url" == "$prefix"* ]] || die "url de ${platform} fora da tag v${version}: ${url}"
    name="${url#"$prefix"}"
    [[ -n "$name" && "$name" != */* && "$name" != *..* ]] || die "nome de ativo inválido em ${platform}: ${name}"
    [[ -f "${dir}/${name}" ]] || die "ativo ${name} (${platform}) ausente em ${dir}"
    actual="$(sha256_of "${dir}/${name}")"
    [[ "$actual" == "$sha" ]] || die "SHA-256 de ${name} diverge: manifesto=${sha} real=${actual}"
  done < <(jq -r '.platforms | to_entries[] | [.key, .value.url, .value.sha256] | @tsv' "$manifest")

  echo "Manifesto v${version} verificado: assinatura, URLs e SHA-256 de ${count} plataformas."
}

main "$@"
