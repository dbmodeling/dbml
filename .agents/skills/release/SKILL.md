---
name: release
description: Prepara uma nova versão do DB Modeling (bump de versão, build release sincronizado, .vsix, commit). Só quando o usuário pedir explicitamente "/release <versão>".
disable-model-invocation: true
---
# release

Argumento: a nova versão (`X.Y.Z`). Sem argumento, pergunte qual é.

1. **Pré-condições:** working tree limpo (`git status --short` vazio). Se não estiver, pare e avise.
2. **Bump:**
   - `version = "..."` em `[workspace.package]` no `Cargo.toml` da raiz. Os crates herdam via `version.workspace`.
   - `cargo check --workspace` para atualizar o `Cargo.lock`.
   - `vscode-extension/package.json` → `"version"`. Hoje ela está fora de sincronia com o workspace: pergunte ao usuário se deve igualar antes de mudar.
3. **Verificar:** execute a skill `check`. Qualquer ❌ → pare.
4. **Build:** `./scripts/sync-build.sh --release --package` (gera wasm release, bundle web em `crates/cli/web/`, `.vsix`).
5. **Commit** (mensagem em pt-BR): `chore: bump version to X.Y.Z and sync web/wasm bundle`.
6. **Tag/push:** o workflow `.github/workflows/release.yml` dispara com tag. **Não** crie tag nem dê push sem confirmação explícita do usuário. Sugira o comando: `git tag vX.Y.Z && git push origin main vX.Y.Z`.

Responda em pt-BR com: versão anterior → nova, arquivos alterados, resultado do check/build e o comando de tag sugerido.
