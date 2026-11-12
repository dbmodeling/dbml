# 04 — Stack e bibliotecas

Versões consultadas no crates.io em **2026-10-24**. Fixar versões exatas no `Cargo.toml`
(o ecossistema GPUI quebra API com frequência).

## UI

| Crate | Versão | Papel | Notas |
|---|---|---|---|
| `gpui-pre` | 0.3.9 | Framework de UI (Zed) | Snapshots oficiais do GPUI publicados pela Zed Industries. O crate `gpui` (0.2.2) está parado desde 2025-10 |
| `gpui-kit` | 0.8.0 | Meta-crate da Longbridge | Re-exporta GPUI + `gpui-base` + `gpui-component` + assets; **depende de `gpui-pre =0.3.9`** — deixar ele ditar a versão do GPUI |
| `gpui-component` | 0.8.0 | 75+ componentes estilo shadcn | **Code editor** (tree-sitter + diagnósticos/completion/hover estilo LSP, 200K linhas), dock com painéis serializáveis, tabelas e listas virtuais, temas, ícones Lucide, AccessKit |

**Recomendação**: depender de `gpui-kit` e não de `gpui-pre` direto, para evitar
conflito de versões. Alternativa a avaliar: `gpuikit` (iamnbutler, 0.9.0) — menor e
menos completo. Avaliar também o backend opcional `gpui-fast`.

O que vem "de graça" do `gpui-component` e economiza meses:
editor de DBML com highlight, painel de diagnósticos, dock/split (editor | canvas |
inspector), command palette/menus, inputs, selects, popovers, tooltips, temas claro/escuro.

O que **teremos que construir**: o canvas do diagrama (elemento customizado).

## Linguagem / parsing

| Crate | Versão | Papel |
|---|---|---|
| `logos` | 0.16.1 | Lexer DBML (rápido, derive) |
| `rowan` | 0.17.0 estável (0.18 está em alpha) | CST sem perdas estilo rust-analyzer |
| `dbml-rs` | 1.0.1 | Oráculo de conformidade nos testes (não no caminho principal) |
| `tree-sitter` | 0.26.x (alinhar com o `gpui-component`) | Highlight no editor; usar/forkar uma gramática `tree-sitter-dbml` da comunidade (verificar qualidade) |
| `ariadne` ou `miette` | 0.6 / 7.6 | Diagnósticos bonitos na CLI/testes (na UI usamos o próprio editor) |

Alternativa considerada para o parser: `chumsky` (1.0 ainda em alpha) — elegante, mas
recuperação de erros + CST sem perdas é mais previsível escrevendo à mão.

## Grafo, layout e geometria

| Crate | Versão | Papel |
|---|---|---|
| `petgraph` | 0.8.3 | Grafo de relações (ciclos, componentes, ordenação) |
| `rust-sugiyama` / `layout-rs` | avaliar | Layout em camadas; possivelmente implementar o nosso |
| `kurbo` | 0.13.1 | Curvas bezier, hit-test de arestas, geometria 2D |
| `lyon` | 1.0.19 | (opcional) tesselação, se os paths do GPUI não bastarem |

## Export (PNG, JPEG, Mermaid)

| Crate | Versão | Papel |
|---|---|---|
| `resvg` (+ `usvg`, `tiny-skia`) | 0.48.1 / 0.12.0 | Rasterizar o SVG gerado pela cena; fontes embarcadas via `fontdb` |
| `image` | 0.25.10 | Encode PNG e JPEG (qualidade configurável), sem features desnecessárias |
| geração própria | — | Cena → SVG; `Schema` → Mermaid `erDiagram` |
| `arboard` | avaliar | Copiar imagem para a área de transferência (se o GPUI não cobrir imagens) |

Fora de escopo: SQL (`sqlparser`) e banco (`sqlx`) foram removidos da stack.

## Infra do app

| Crate | Versão | Papel |
|---|---|---|
| `serde` + `serde_json` | 1.0 | Sidecar de layout (JSON) e configurações |
| `notify` | 8.x estável (9 em rc) | Detectar alterações externas no arquivo |
| `rfd` | 0.17.2 | Diálogos nativos de abrir/salvar (verificar se o GPUI já expõe `prompt_for_paths`, que dispensa) |
| `directories` | 6.0.0 | Pastas de config/cache por SO |
| `nucleo` | 0.5.0 | Fuzzy matching (command palette, busca de tabelas/tipos) |
| `thiserror` / `anyhow` | 2.x / 1.x | Erros |
| `tracing` + `tracing-subscriber` | 0.1 | Logs |

## Testes e qualidade

`insta` (1.49, snapshots), `proptest` (propriedades do round-trip texto↔UI),
`criterion` ou `divan` (benchmarks de parser/layout), `cargo-nextest`, `clippy`, `rustfmt`.

## Empacotamento e CI

| Ferramenta | Papel |
|---|---|
| `cargo-packager` (avaliar) ou `cargo-wix` + `cargo-deb` | Instalador Windows (MSI/NSIS) e `.deb`/AppImage para Ubuntu |
| GitHub Actions | Matriz `windows-latest` + `ubuntu-24.04`: build, testes, clippy, artefatos |
| `cargo-xwin` (avaliar) | Compilar para Windows a partir do WSL/Linux |

Dependências de sistema no Ubuntu (para o GPUI): headers de Vulkan, X11/xkbcommon,
Wayland, fontconfig/freetype — a lista exata vem do spike 1.

## Toolchain

`rustc 1.99.1` instalado. Ambiente de desenvolvimento atual: **WSL2** — ver R1 em `riscos.md`.


## Adições reais no `Cargo.toml` e Frontend

- **Parsing e CST:** `rowan =0.18.0-alpha.2` (`logos =0.16.1`).
- **WebAssembly & CLI Web:** `wasm-bindgen 0.2`, `serde-wasm-bindgen 0.6`, `axum 0.8` (`ws`), `tokio 1`, `rust-embed =8.13.0`, `clap 4.5`, `opener 0.7`, `colored 3.0`, `mime_guess 2.0`.
- **Desktop & LSP:** `lsp-types =0.97.0`, `embed-resource =3.0.12`.
- **Testes & Lints:** `insta =1.49.0`, `proptest =1.12.0`, `[workspace.lints]` (`clippy::pedantic`, `unsafe_op_in_unsafe_fn`).
- **Frontend & Extensão:** `packages/canvas-web` (TypeScript 5.7, Vite 6, Vitest) e `vscode-extension/` sincronizados por `./scripts/sync-build.sh`.
