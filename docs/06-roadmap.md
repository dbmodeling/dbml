# 06 — Roadmap

Escopo fixo: modelagem visual + DBML; export PNG, JPEG e Mermaid. Alvos: Windows e Ubuntu.

## Fase 0 — Spikes (validar riscos antes de arquitetar)
Protótipos descartáveis, cada um respondendo uma pergunta:

1. **GPUI nos alvos**: hello world com `gpui-kit` rodando no **Ubuntu** (Wayland e X11),
   no **Windows** nativo e, se possível, no WSLg para o dia a dia do desenvolvimento (R1).
2. **Canvas**: 500 "tabelas" com zoom/pan/arrastar a 120 fps; arestas bezier; LOD (R2).
3. **Editor**: editor do `gpui-component` com highlight DBML via tree-sitter e
   diagnósticos/completion injetados por nós (R3).
4. **Round-trip**: parser mínimo `logos`+`rowan` com um comando `AddColumn` gerando
   patch textual que preserva comentários (R4).
5. **Export**: cena simples → SVG → PNG/JPEG com `resvg` usando a mesma fonte do app,
   comparada lado a lado com o canvas (R8).

## Fase 1 — Fundação (sem UI rica)
- Workspace de crates, CI com matriz Windows + Ubuntu (fmt, clippy, nextest).
- `schema`, `dbml-syntax`, `dbml-analysis` com cobertura DBML do MVP.
- Corpus de testes + conformidade com `dbml-rs`.
- CLI de desenvolvimento: `dbm check arquivo.dbml`, `dbm mermaid arquivo.dbml`.

## Fase 2 — MVP visual
- Janela com dock: editor | canvas | inspector.
- Canvas: render de tabelas e refs, pan/zoom, mover, seleção, auto-layout básico.
- Sincronização texto → canvas ao vivo; diagnósticos.
- Sidecar de layout; abrir/salvar; autosave.

## Fase 3 — Modelagem só por UI
- Criar/editar tabelas, colunas, tipos, flags, refs, enums inline.
- `dbml-edit` (UI → patch textual mínimo); undo unificado.
- Command palette, Ctrl+P, atalhos, inferência por nome.

## Fase 4 — Export e polimento
- `scene` + `export`: PNG (transparente/escala), JPEG (qualidade), Mermaid `erDiagram`
  com aviso de perdas; painel de export com preview; copiar para a área de transferência.
- Views, TableGroups, focus mode, minimap, temas claro/escuro.
- Roteamento ortogonal de arestas; auto-layout de qualidade.
- Instaladores: MSI/NSIS (Windows), `.deb`/AppImage (Ubuntu).

## Futuro (não comprometido)
- Export SVG (sai quase de graça do pipeline de imagem).
- Documentação gerada (HTML estilo dbdocs).
- LSP standalone para DBML (reaproveitando `dbml-analysis`) para VS Code/Zed.
- Colaboração (CRDT), plugins, macOS.


## Estado atual implementado

- **Núcleo Rust puro:** `schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit` e `layout` implementados com `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, testes unitários, snapshots (`insta`) e `proptest`.
- **3 alvos entregues:** App Desktop (`crates/app`), CLI Web (`crates/cli` / `dbml` com Axum/WebSocket) e Extensão VS Code (`vscode-extension/` + `crates/dbml-wasm` + `packages/canvas-web`), sincronizados por `./scripts/sync-build.sh`.
- **Empacotamento:** Windows (`.exe` Inno Setup), macOS (`.dmg`/`.pkg`), Debian/Ubuntu (`.deb`), Fedora (`.rpm`) e `scripts/install.sh`.
