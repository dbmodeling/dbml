#!/usr/bin/env bash
# ==============================================================================
# Gera o tarball de usuário do Linux: dist/db-modeling-linux-x86_64.tar.gz
#
# É o artefato que o `install.sh` (modo padrão, sem sudo) e o auto-update do app
# instalam em ~/.local. Contém a pasta `db-modeling.app`, com a MESMA entrega dos
# pacotes .deb/.rpm (binário, .desktop, ícones, tipo MIME, licença), para o
# `rsync --delete` da atualização não apagar nada que o pacote já entregava:
#
#   db-modeling.app/bin/db-modeling
#   db-modeling.app/share/applications/db-modeling.desktop
#   db-modeling.app/share/icons/hicolor/{scalable,256x256}/apps/db-modeling.{svg,png}
#   db-modeling.app/share/mime/packages/db-modeling.xml
#   db-modeling.app/share/doc/db-modeling/copyright
#
# (os pacotes não entregam a CLI `dbml`, então o tarball também não.)
#
# Variáveis opcionais:
#   DBML_APP_BIN  executável já compilado (padrão: target/release/app, compilado
#                 aqui se faltar). Útil em testes, com um binário de mentira.
#   DIST_DIR      pasta de saída (padrão: <repo>/dist)
#   SOURCE_DATE_EPOCH  data fixa dos arquivos, para o tarball ser reproduzível
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="db-modeling"
APP_DIR_NAME="${APP_NAME}.app"
TARBALL_NAME="${APP_NAME}-linux-x86_64.tar.gz"
DIST_DIR="${DIST_DIR:-${ROOT_DIR}/dist}"
APP_BIN="${DBML_APP_BIN:-${ROOT_DIR}/target/release/app}"
BRAND_DIR="${ROOT_DIR}/crates/app/assets/brand"
FALLBACK_EPOCH=1700000000

if [ "$(uname -s)" != "Linux" ]; then
    echo "Este script só gera o tarball no Linux." >&2
    exit 1
fi

if [ -z "${DBML_APP_BIN:-}" ] && [ ! -x "$APP_BIN" ]; then
    echo "==> Compilando o binário release (cargo build --release -p app)..."
    cargo build --release -p app
fi
if [ ! -x "$APP_BIN" ]; then
    echo "Executável não encontrado ou sem permissão de execução: ${APP_BIN}" >&2
    exit 1
fi

STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT
APP="${STAGE_ROOT}/${APP_DIR_NAME}"

echo "==> Montando ${APP_DIR_NAME}..."
mkdir -p "${APP}/bin" \
    "${APP}/share/applications" \
    "${APP}/share/icons/hicolor/scalable/apps" \
    "${APP}/share/icons/hicolor/256x256/apps" \
    "${APP}/share/mime/packages" \
    "${APP}/share/doc/${APP_NAME}"

cp "$APP_BIN" "${APP}/bin/${APP_NAME}"
strip "${APP}/bin/${APP_NAME}" 2>/dev/null || true
chmod 0755 "${APP}/bin/${APP_NAME}"

cp "${ROOT_DIR}/LICENSE" "${APP}/share/doc/${APP_NAME}/copyright"
cp "${BRAND_DIR}/icon.svg" "${APP}/share/icons/hicolor/scalable/apps/${APP_NAME}.svg"
cp "${BRAND_DIR}/icon-256.png" "${APP}/share/icons/hicolor/256x256/apps/${APP_NAME}.png"

# Igual ao dos pacotes; o install.sh troca `Exec=`/`Icon=` por caminhos absolutos.
cat > "${APP}/share/applications/${APP_NAME}.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=dbmodeling
GenericName=Visual Database Modeler (DBML)
Comment=Visual relational database schema modeling with DBML
Exec=${APP_NAME} %F
Icon=${APP_NAME}
Terminal=false
Categories=Development;Database;IDE;
MimeType=application/x-dbml;text/x-dbml;
StartupWMClass=db-modeling
Keywords=dbml;database;schema;erd;diagram;sql;
EOF

cat > "${APP}/share/mime/packages/${APP_NAME}.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-dbml">
    <comment>DBML Schema</comment>
    <glob pattern="*.dbml"/>
    <sub-class-of type="text/plain"/>
  </mime-type>
</mime-info>
EOF

mkdir -p "$DIST_DIR"
OUTPUT="${DIST_DIR}/${TARBALL_NAME}"
EPOCH="${SOURCE_DATE_EPOCH:-$FALLBACK_EPOCH}"
echo "==> Gerando ${OUTPUT}..."
# Ordem, dono e data fixos + `gzip -n` (sem nome/data no cabeçalho): mesmo conteúdo, mesmo hash.
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${EPOCH}" \
    -C "$STAGE_ROOT" -cf - "$APP_DIR_NAME" | gzip -n > "$OUTPUT"

echo "==> Tarball gerado com sucesso:"
echo "    ${OUTPUT}"
tar -tzf "$OUTPUT"
