#!/usr/bin/env bash
# ==============================================================================
# Validação multiplataforma local (Cross-Target Clippy + Simetria de #[cfg]).
#
# Roda localmente no Linux o `cargo clippy -- -D warnings` para:
#   1. Windows (`x86_64-pc-windows-gnu`) nas crates multiplataforma sem C/ring
#      (inclui `updater` com `windows_acl` e `auto-update-helper` com `win.rs`);
#   2. macOS (`aarch64-apple-darwin`) nas crates multiplataforma;
#   3. WebAssembly (`wasm32-unknown-unknown`) em `dbml-wasm`;
#   4. Auditoria de simetria de `#[cfg(...)]` em `crates/app/src/`.
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CROSS_CRATES=(
  -p schema
  -p dbml-syntax
  -p dbml-analysis
  -p dbml-edit
  -p layout
  -p sql-import
  -p diagram-export
  -p atomic-write
  -p updater
  -p auto-update-helper
)

installed="$(rustup target list --installed)"

has_target() {
  grep -qx "$1" <<< "$installed"
}

if has_target "x86_64-pc-windows-gnu"; then
  echo "==> [cross-check] Clippy para Windows (x86_64-pc-windows-gnu)..."
  cargo clippy --locked "${CROSS_CRATES[@]}" --target x86_64-pc-windows-gnu --all-targets -- -D warnings
else
  echo "Aviso: target x86_64-pc-windows-gnu não instalado (rode: rustup target add x86_64-pc-windows-gnu)" >&2
  exit 1
fi

if has_target "aarch64-apple-darwin"; then
  echo "==> [cross-check] Clippy para macOS (aarch64-apple-darwin)..."
  cargo clippy --locked "${CROSS_CRATES[@]}" --target aarch64-apple-darwin --all-targets -- -D warnings
else
  echo "Aviso: target aarch64-apple-darwin não instalado (rode: rustup target add aarch64-apple-darwin)" >&2
  exit 1
fi

if has_target "wasm32-unknown-unknown"; then
  echo "==> [cross-check] Clippy para Wasm (wasm32-unknown-unknown)..."
  cargo clippy --locked -p dbml-wasm --target wasm32-unknown-unknown -- -D warnings
fi

echo "==> [cross-check] Verificando simetria de #[cfg] em crates/app..."
python3 - <<'PY'
import pathlib, re, sys

app_src = pathlib.Path("crates/app/src")
files = {p.name: p.read_text(encoding="utf-8") for p in app_src.rglob("*.rs")}

# Verifica se constantes com #[cfg(any(target_os = "linux", test))] são usadas
# em testes que rodam fora do Linux (isto é, fora de install_linux.rs).
const_text = files["constants.rs"]
pattern = re.compile(
    r'#\[cfg\(any\(target_os\s*=\s*"linux",\s*test\)\)\]\s*pub\s+const\s+([A-Z0-9_]+)\b'
)
errors = []
for match in pattern.finditer(const_text):
    name = match.group(1)
    used_outside_linux_only = False
    for fname, content in files.items():
        if fname in ("constants.rs", "install_linux.rs", "updater_ui.rs"):
            continue
        if re.search(r"\b" + re.escape(name) + r"\b", content):
            used_outside_linux_only = True
            break
    if not used_outside_linux_only:
        errors.append(
            f"crates/app/src/constants.rs: `{name}` tem #[cfg(any(target_os = \"linux\", test))], "
            f"mas só é usada em install_linux.rs (#[cfg(target_os = \"linux\")]). "
            f"Em `cargo clippy --all-targets` no Windows/macOS ela causará `dead_code`!"
        )

if errors:
    for err in errors:
        print(f"❌ {err}", file=sys.stderr)
    sys.exit(1)
PY

echo "✓ Validação multiplataforma local concluída sem erros."
