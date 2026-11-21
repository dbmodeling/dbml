#!/usr/bin/env bash
# Utilitário para atualizar a versão unificada do projeto (Cargo.toml + package.json).
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Uso: $0 <nova-versao>"
    echo "Exemplo: $0 0.1.1"
    exit 1
fi

NEW_VERSION="$1"

# Valida formato semver X.Y.Z
if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Erro: '$NEW_VERSION' não parece ser uma versão SemVer válida (ex: 0.1.1 ou 1.0.0)."
    exit 1
fi

cd "$(dirname "$0")/.."

CURRENT_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
echo "==> Versão atual: $CURRENT_VERSION"
echo "==> Atualizando versão no Cargo.toml raiz para $NEW_VERSION..."

sed -i.bak -E "s/^(version = \")[^\"]+(\")/\1$NEW_VERSION\2/" Cargo.toml
rm -f Cargo.toml.bak

for pkg in vscode-extension/package.json packages/canvas-web/package.json; do
    echo "==> Atualizando versão em $pkg..."
    (cd "$(dirname "$pkg")" && npm version "$NEW_VERSION" --no-git-tag-version --allow-same-version >/dev/null)
done

echo "==> Formatando código e validando integridade (fmt + clippy + test)..."
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

echo ""
echo "=========================================================="
echo " Versão atualizada com sucesso para $NEW_VERSION!"
echo ""
echo " Para compilar e gerar os pacotes atualizados:"
echo "   Fedora (.rpm):   ./scripts/build-fedora.sh"
echo "   Debian (.deb):   ./scripts/build-deb.sh"
echo "   Windows (.exe):  ./scripts/build-windows.sh"
echo "   macOS (.dmg):    ./scripts/build-macos.sh"
echo "=========================================================="
