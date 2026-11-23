#!/usr/bin/env bash
# ==============================================================================
# Gera o manifesto de auto-update `latest.json` (schema 1) a partir dos ativos
# reais de `release-dist/`. Formato EXATO esperado por `crates/updater`
# (`Manifest::parse` + `asset_for`).
#
# Entradas SEMPRE por variável de ambiente (nada de interpolação no shell):
#   RELEASE_TAG        tag da release, ex.: v0.4.0 (obrigatória; só canal estável)
#   RELEASE_DIST_DIR   diretório com os ativos consolidados (obrigatória)
#   MANIFEST_OUTPUT    caminho do `latest.json` a gerar (obrigatória)
#   CARGO_PKG_VERSION  versão do workspace; se ausente, é lida de
#                      [workspace.package] em Cargo.toml (CARGO_MANIFEST_PATH)
#   MACOS_TEAM_ID      Team ID da Apple; só informar se o build do macOS foi assinado
#   RELEASE_REPO       dono/repositório (padrão: o repositório fixado no cliente)
#
# Falha (exit != 0) se: faltar entrada ou ativo esperado, a tag tiver sufixo de
# pré-release/build (o canal é só `stable`), ou tag, `CARGO_PKG_VERSION` e a
# versão do manifesto divergirem.
# ==============================================================================
set -euo pipefail

# --- Constantes (nomes dos ativos = nomes reais produzidos pelos jobs de build) ---
readonly DEFAULT_RELEASE_REPO="dbmodeling/dbml"
readonly MANIFEST_SCHEMA=1
readonly PLATFORM_WINDOWS="windows-x86_64"
readonly PLATFORM_MACOS="macos-aarch64"
readonly PLATFORM_LINUX="linux-x86_64"
readonly PLATFORM_LINUX_DEB="linux-x86_64-deb"
readonly PLATFORM_LINUX_RPM="linux-x86_64-rpm"
readonly ASSET_WINDOWS="db-modeling-windows-setup.exe"
readonly ASSET_MACOS="db-modeling-macos-arm64.dmg"
readonly ASSET_LINUX="db-modeling-linux-x86_64.tar.gz"
readonly ASSET_LINUX_DEB="db-modeling-linux-amd64.deb"
readonly ASSET_LINUX_RPM="db-modeling-linux-x86_64.rpm"
readonly SEMVER_NUMBER='(0|[1-9][0-9]*)'
readonly STABLE_TAG_REGEX="^v(${SEMVER_NUMBER}\\.${SEMVER_NUMBER}\\.${SEMVER_NUMBER})\$"
readonly SUFFIXED_TAG_REGEX="^v${SEMVER_NUMBER}\\.${SEMVER_NUMBER}\\.${SEMVER_NUMBER}[-+]"
readonly TEAM_ID_REGEX='^[A-Z0-9]{10}$'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

die() {
  echo "::error::generate-update-manifest: $*" >&2
  exit 1
}

require_env() {
  local name="$1"
  [[ -n "${!name:-}" ]] || die "variável de ambiente ${name} obrigatória e vazia."
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

workspace_version() {
  local manifest="$1"
  [[ -f "$manifest" ]] || die "Cargo.toml não encontrado: ${manifest}"
  awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && /^version[[:space:]]*=/ {
      gsub(/^version[[:space:]]*=[[:space:]]*"|".*$/, "")
      print
      exit
    }' "$manifest"
}

main() {
  command -v jq >/dev/null 2>&1 || die "jq é obrigatório."
  require_env RELEASE_TAG
  require_env RELEASE_DIST_DIR
  require_env MANIFEST_OUTPUT

  local tag="$RELEASE_TAG"
  local repo="${RELEASE_REPO:-$DEFAULT_RELEASE_REPO}"

  # Canal único `stable`: sufixo de pré-release (-rc.1) ou build (+meta) não gera manifesto.
  if [[ "$tag" =~ $SUFFIXED_TAG_REGEX ]]; then
    die "a tag '${tag}' é pré-release/build; só tags estáveis vMAJOR.MINOR.PATCH geram manifesto."
  fi
  [[ "$tag" =~ $STABLE_TAG_REGEX ]] || die "tag inválida: '${tag}' (esperado vMAJOR.MINOR.PATCH)."
  local version="${tag#v}"

  local cargo_version="${CARGO_PKG_VERSION:-}"
  if [[ -z "$cargo_version" ]]; then
    cargo_version="$(workspace_version "${CARGO_MANIFEST_PATH:-$ROOT_DIR/Cargo.toml}")"
  fi
  [[ -n "$cargo_version" ]] || die "não foi possível determinar CARGO_PKG_VERSION."
  [[ "$cargo_version" == "$version" ]] \
    || die "versão divergente: tag '${tag}' (${version}) != CARGO_PKG_VERSION '${cargo_version}'."

  local team_id="${MACOS_TEAM_ID:-}"
  if [[ -n "$team_id" && ! "$team_id" =~ $TEAM_ID_REGEX ]]; then
    die "MACOS_TEAM_ID inválido (esperado 10 caracteres A-Z0-9)."
  fi

  [[ -d "$RELEASE_DIST_DIR" ]] || die "diretório de ativos inexistente: ${RELEASE_DIST_DIR}"
  local asset
  for asset in "$ASSET_WINDOWS" "$ASSET_MACOS" "$ASSET_LINUX" "$ASSET_LINUX_DEB" "$ASSET_LINUX_RPM"; do
    [[ -s "${RELEASE_DIST_DIR}/${asset}" ]] || die "ativo esperado ausente ou vazio: ${asset}"
  done

  local base_url="https://github.com/${repo}/releases/download/${tag}"
  local notes_url="https://github.com/${repo}/releases/tag/${tag}"
  local sha_windows sha_macos sha_linux sha_linux_deb sha_linux_rpm
  sha_windows="$(sha256_of "${RELEASE_DIST_DIR}/${ASSET_WINDOWS}")"
  sha_macos="$(sha256_of "${RELEASE_DIST_DIR}/${ASSET_MACOS}")"
  sha_linux="$(sha256_of "${RELEASE_DIST_DIR}/${ASSET_LINUX}")"
  sha_linux_deb="$(sha256_of "${RELEASE_DIST_DIR}/${ASSET_LINUX_DEB}")"
  sha_linux_rpm="$(sha256_of "${RELEASE_DIST_DIR}/${ASSET_LINUX_RPM}")"

  local out_dir tmp
  out_dir="$(dirname "$MANIFEST_OUTPUT")"
  mkdir -p "$out_dir"
  tmp="$(mktemp "${out_dir}/.latest.json.XXXXXX")"
  trap 'rm -f "$tmp"' EXIT

  # `jq` monta o JSON a partir de --arg: nenhum valor externo passa pelo parser do shell.
  jq -n \
    --argjson schema "$MANIFEST_SCHEMA" \
    --arg version "$version" \
    --arg notes_url "$notes_url" \
    --arg k_windows "$PLATFORM_WINDOWS" --arg u_windows "${base_url}/${ASSET_WINDOWS}" --arg s_windows "$sha_windows" \
    --arg k_macos "$PLATFORM_MACOS" --arg u_macos "${base_url}/${ASSET_MACOS}" --arg s_macos "$sha_macos" \
    --arg team_id "$team_id" \
    --arg k_linux "$PLATFORM_LINUX" --arg u_linux "${base_url}/${ASSET_LINUX}" --arg s_linux "$sha_linux" \
    --arg k_linux_deb "$PLATFORM_LINUX_DEB" --arg u_linux_deb "${base_url}/${ASSET_LINUX_DEB}" --arg s_linux_deb "$sha_linux_deb" \
    --arg k_linux_rpm "$PLATFORM_LINUX_RPM" --arg u_linux_rpm "${base_url}/${ASSET_LINUX_RPM}" --arg s_linux_rpm "$sha_linux_rpm" \
    '{
      schema: $schema,
      version: $version,
      notes_url: $notes_url,
      platforms: {
        ($k_windows): { url: $u_windows, sha256: $s_windows },
        ($k_macos): ({ url: $u_macos, sha256: $s_macos }
                     + (if $team_id == "" then {} else { team_id: $team_id } end)),
        ($k_linux): { url: $u_linux, sha256: $s_linux },
        ($k_linux_deb): { url: $u_linux_deb, sha256: $s_linux_deb },
        ($k_linux_rpm): { url: $u_linux_rpm, sha256: $s_linux_rpm }
      }
    }' > "$tmp"

  # Reconferência do arquivo escrito: versão do manifesto == tag == Cargo.
  local written
  written="$(jq -r '.version' "$tmp")"
  [[ "$written" == "$version" && "$written" == "$cargo_version" ]] \
    || die "versão do manifesto '${written}' diverge da tag/Cargo."

  chmod 0644 "$tmp"
  mv -f "$tmp" "$MANIFEST_OUTPUT"
  trap - EXIT
  echo "Manifesto gerado: ${MANIFEST_OUTPUT} (versão ${version})"
}

main "$@"
