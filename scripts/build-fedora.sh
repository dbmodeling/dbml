#!/usr/bin/env bash
set -euo pipefail

# Build .rpm package for Fedora / RHEL / openSUSE
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
ARCH="$(uname -m)"
PKG_NAME="db-modeling"
DIST_DIR="${ROOT_DIR}/dist"
RPMBUILD_DIR="${ROOT_DIR}/target/rpmbuild"

echo "==> Checking Fedora/RPM build dependencies..."
if ! command -v rpmbuild >/dev/null 2>&1; then
    echo "Error: 'rpmbuild' not found. Install it on Fedora with:"
    echo "  sudo dnf install rpm-build rpmdevtools"
    exit 1
fi

echo "==> Building binary in release mode (v${VERSION})..."
cargo build --release -p app

echo "==> Preparing rpmbuild tree at ${RPMBUILD_DIR}..."
rm -rf "${RPMBUILD_DIR}"
mkdir -p "${RPMBUILD_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

STAGE_ROOT="${RPMBUILD_DIR}/SOURCES/${PKG_NAME}-${VERSION}"
mkdir -p "${STAGE_ROOT}/usr/bin"
mkdir -p "${STAGE_ROOT}/usr/share/applications"
mkdir -p "${STAGE_ROOT}/usr/share/icons/hicolor/scalable/apps"
mkdir -p "${STAGE_ROOT}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${STAGE_ROOT}/usr/share/mime/packages"
mkdir -p "${STAGE_ROOT}/usr/share/licenses/${PKG_NAME}"

# O app se atualiza sozinho (pkexec dnf install -y); este pacote NÃO instala mais o arquivo
# /usr/lib/db-modeling/update-explanation que desligava o auto-update.
cp "${ROOT_DIR}/target/release/app" "${STAGE_ROOT}/usr/bin/${PKG_NAME}"
strip "${STAGE_ROOT}/usr/bin/${PKG_NAME}" || true
chmod 0755 "${STAGE_ROOT}/usr/bin/${PKG_NAME}"

cp "${ROOT_DIR}/LICENSE" "${STAGE_ROOT}/usr/share/licenses/${PKG_NAME}/LICENSE"
cp "${ROOT_DIR}/crates/app/assets/brand/icon.svg" "${STAGE_ROOT}/usr/share/icons/hicolor/scalable/apps/${PKG_NAME}.svg"
cp "${ROOT_DIR}/crates/app/assets/brand/icon-256.png" "${STAGE_ROOT}/usr/share/icons/hicolor/256x256/apps/${PKG_NAME}.png"

cat > "${STAGE_ROOT}/usr/share/applications/${PKG_NAME}.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=dbmodeling
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

cat > "${STAGE_ROOT}/usr/share/mime/packages/${PKG_NAME}.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-dbml">
    <comment>DBML Schema</comment>
    <glob pattern="*.dbml"/>
    <sub-class-of type="text/plain"/>
  </mime-type>
</mime-info>
EOF

SPEC_FILE="${RPMBUILD_DIR}/SPECS/${PKG_NAME}.spec"
cat > "${SPEC_FILE}" <<EOF
%global debug_package %{nil}
%define _build_id_links none

Name:           ${PKG_NAME}
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Visual Database Modeler powered by DBML
License:        Apache-2.0
URL:            https://github.com/smovisk/db_modeling
BuildArch:      ${ARCH}
AutoReqProv:    no
Requires:       glibc, libgcc, libstdc++, libxcb, libxkbcommon, libxkbcommon-x11, libwayland-client, vulkan-loader, fontconfig, freetype

%description
Native, high-performance visual database schema editor using DBML as the canonical
format. Features an interactive ER canvas, bidirectional DBML code editor,
auto-layout, and high-resolution diagram export.

%install
mkdir -p %{buildroot}
cp -a ${STAGE_ROOT}/usr %{buildroot}/

%post
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database /usr/share/mime || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi

%files
/usr/bin/${PKG_NAME}
/usr/share/applications/${PKG_NAME}.desktop
/usr/share/icons/hicolor/scalable/apps/${PKG_NAME}.svg
/usr/share/icons/hicolor/256x256/apps/${PKG_NAME}.png
/usr/share/mime/packages/${PKG_NAME}.xml
/usr/share/licenses/${PKG_NAME}/LICENSE
EOF

echo "==> Packaging .rpm with rpmbuild..."
rpmbuild --define "_topdir ${RPMBUILD_DIR}" -bb "${SPEC_FILE}"

mkdir -p "${DIST_DIR}"
RPM_GENERATED="$(find "${RPMBUILD_DIR}/RPMS" -name "*.rpm" | head -n 1)"
OUTPUT_RPM="${DIST_DIR}/db-modeling-v${VERSION}-${ARCH}.rpm"

cp "${RPM_GENERATED}" "${DIST_DIR}/"
cp "${RPM_GENERATED}" "${OUTPUT_RPM}"

echo "==> Fedora (.rpm) package generated successfully:"
echo "    ${OUTPUT_RPM}"
