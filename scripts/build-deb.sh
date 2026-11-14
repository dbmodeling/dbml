#!/usr/bin/env bash
set -euo pipefail

# Build .deb package for Ubuntu / Debian
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
PKG_NAME="db-modeling"
DEB_NAME="${PKG_NAME}_${VERSION}_${ARCH}"
STAGE_DIR="${ROOT_DIR}/target/deb/${DEB_NAME}"
DIST_DIR="${ROOT_DIR}/dist"

echo "==> Building binary in release mode (v${VERSION})..."
cargo build --release -p app

echo "==> Preparing package structure at ${STAGE_DIR}..."
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/DEBIAN"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${STAGE_DIR}/usr/share/mime/packages"
mkdir -p "${STAGE_DIR}/usr/share/doc/${PKG_NAME}"

cp "${ROOT_DIR}/target/release/app" "${STAGE_DIR}/usr/bin/${PKG_NAME}"
strip "${STAGE_DIR}/usr/bin/${PKG_NAME}" || true
chmod 0755 "${STAGE_DIR}/usr/bin/${PKG_NAME}"

cp "${ROOT_DIR}/LICENSE" "${STAGE_DIR}/usr/share/doc/${PKG_NAME}/copyright"
cp "${ROOT_DIR}/crates/app/assets/brand/icon.svg" "${STAGE_DIR}/usr/share/icons/hicolor/scalable/apps/${PKG_NAME}.svg"
cp "${ROOT_DIR}/crates/app/assets/brand/icon-256.png" "${STAGE_DIR}/usr/share/icons/hicolor/256x256/apps/${PKG_NAME}.png"

cat > "${STAGE_DIR}/usr/share/applications/${PKG_NAME}.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=DB Modeling
GenericName=Visual Database Modeler (DBML)
Comment=Visual relational database schema modeling with DBML
Exec=${PKG_NAME} %F
Icon=${PKG_NAME}
Terminal=false
Categories=Development;Database;IDE;
MimeType=application/x-dbml;text/x-dbml;
StartupWMClass=db-modeling
Keywords=dbml;database;schema;erd;diagram;sql;
EOF

cat > "${STAGE_DIR}/usr/share/mime/packages/${PKG_NAME}.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-dbml">
    <comment>DBML Schema</comment>
    <glob pattern="*.dbml"/>
    <sub-class-of type="text/plain"/>
  </mime-type>
</mime-info>
EOF

INSTALLED_SIZE="$(du -sk "${STAGE_DIR}/usr" | cut -f1)"

cat > "${STAGE_DIR}/DEBIAN/control" <<EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: devel
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_SIZE}
Depends: libc6, libgcc-s1, libstdc++6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libvulkan1, libfontconfig1, libfreetype6
Maintainer: DB Modeling Team <dev@dbmodeling.local>
Homepage: https://github.com/db-modeling/db-modeling
Description: Visual Database Modeler powered by DBML
 Native, fast, visual database schema editor using DBML as the canonical
 format. Features an interactive ER canvas, bidirectional DBML code editor,
 auto-layout, and high-resolution diagram export.
EOF

cat > "${STAGE_DIR}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database /usr/share/mime || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
EOF
chmod 0755 "${STAGE_DIR}/DEBIAN/postinst"

mkdir -p "${DIST_DIR}"
OUTPUT_DEB="${DIST_DIR}/${DEB_NAME}.deb"
dpkg-deb --build --root-owner-group "${STAGE_DIR}" "${OUTPUT_DEB}"

# Also copy with standard release asset name
cp "${OUTPUT_DEB}" "${DIST_DIR}/db-modeling-v${VERSION}-${ARCH}.deb"

echo "==> Package generated successfully:"
echo "    ${OUTPUT_DEB}"
echo "    ${DIST_DIR}/db-modeling-v${VERSION}-${ARCH}.deb"
