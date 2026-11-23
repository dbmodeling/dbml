#!/usr/bin/env bash
# ==============================================================================
# Assina o manifesto de auto-update com minisign PREHASHED (`rsign sign` padrão,
# sem `-l`): o cliente (`allow_legacy = false`) recusa assinaturas legadas.
#
# Uso: ./scripts/sign-update-manifest.sh <latest.json>   -> grava <latest.json>.sig
#
# Entrada por ambiente:
#   UPDATER_SIGNING_KEY   conteúdo do arquivo de chave secreta do rsign/minisign,
#                         SEM senha (a chave é gerada com `rsign generate -W`).
#                         Falha se ausente.
# A chave vai para um arquivo temporário 0600 (em diretório 0700), apagado no fim
# (inclusive em erro). Depois de assinar, confere a assinatura com a chave pública
# primária embutida no cliente: um segredo trocado/errado derruba o passo.
# ==============================================================================
set -euo pipefail

readonly SIGNATURE_SUFFIX=".sig"
readonly SECRET_KEY_FILE_NAME="updater-signing.key"
readonly SECRET_KEY_MODE=600

KEY_DIR=""
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/update-public-key.sh
source "${ROOT_DIR}/scripts/lib/update-public-key.sh"

die() {
  echo "::error::sign-update-manifest: $*" >&2
  exit 1
}

main() {
  [[ $# -eq 1 ]] || die "uso: $0 <latest.json>"
  local manifest="$1"
  [[ -f "$manifest" ]] || die "manifesto inexistente: ${manifest}"
  [[ -n "${UPDATER_SIGNING_KEY:-}" ]] \
    || die "o segredo UPDATER_SIGNING_KEY está ausente ou vazio; sem ele não há release com auto-update."
  command -v rsign >/dev/null 2>&1 || die "rsign não encontrado (instale rsign2 0.6.7)."

  local public_key signature key_file
  public_key="$(primary_public_key "$ROOT_DIR")"
  signature="${manifest}${SIGNATURE_SUFFIX}"

  umask 077
  KEY_DIR="$(mktemp -d)"
  trap 'rm -rf "$KEY_DIR"' EXIT
  key_file="${KEY_DIR}/${SECRET_KEY_FILE_NAME}"
  # `printf` é builtin: o segredo não aparece em argv de nenhum processo.
  printf '%s\n' "$UPDATER_SIGNING_KEY" > "$key_file"
  chmod "$SECRET_KEY_MODE" "$key_file"

  rm -f "$signature"
  # Padrão do rsign = prehashed (algoritmo "ED"); -W = chave sem senha, sem prompt de TTY.
  rsign sign -W -s "$key_file" -x "$signature" "$manifest" >/dev/null \
    || die "falha ao assinar (chave inválida ou protegida por senha?)."

  # `rsign verify` sem `-l` recusa assinatura legada, como o cliente.
  if ! rsign verify -q -P "$public_key" -x "$signature" "$manifest"; then
    rm -f "$signature"
    die "a assinatura gerada NÃO valida com a chave pública primária embutida (segredo errado?)."
  fi
  echo "Manifesto assinado: ${signature}"
}

main "$@"
