#!/usr/bin/env bash
# Funções compartilhadas pelos scripts de assinatura/verificação do manifesto de auto-update.
# Fonte única da chave pública primária: `PUBLIC_KEY_PRIMARY` em crates/updater/src/constants.rs
# (a mesma embutida no cliente). Uso: `source scripts/lib/update-public-key.sh`.

readonly UPDATER_CONSTANTS_RS_DEFAULT="crates/updater/src/constants.rs"
readonly PRIMARY_KEY_CONSTANT_NAME="PUBLIC_KEY_PRIMARY"
readonly PUBLIC_KEY_BASE64_REGEX='"RW[A-Za-z0-9+/=]+"'

# Imprime a chave pública primária (linha base64). `UPDATER_PUBLIC_KEY` (env) a substitui; só os
# testes locais usam isso, com um par descartável. O workflow de release nunca o define.
primary_public_key() {
  local root_dir="$1" key constants
  if [[ -n "${UPDATER_PUBLIC_KEY:-}" ]]; then
    printf '%s\n' "$UPDATER_PUBLIC_KEY"
    return 0
  fi
  constants="${UPDATER_CONSTANTS_RS:-${root_dir}/${UPDATER_CONSTANTS_RS_DEFAULT}}"
  # A constante pode quebrar de linha no rustfmt: olha a linha do nome e a seguinte.
  key="$(grep -A1 -- "pub const ${PRIMARY_KEY_CONSTANT_NAME}:" "$constants" \
    | grep -Eo -- "$PUBLIC_KEY_BASE64_REGEX" | head -n 1 | tr -d '"')" || true
  if [[ -z "$key" ]]; then
    echo "::error::chave pública primária não encontrada em ${constants}" >&2
    return 1
  fi
  printf '%s\n' "$key"
}
