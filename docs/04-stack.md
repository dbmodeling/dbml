# 04 — Stack e bibliotecas

As versões são mantidas no [Cargo.toml](../Cargo.toml), no
[Cargo.lock](../Cargo.lock) e nos manifests de cada pacote JavaScript. A
toolchain Rust está definida em [rust-toolchain.toml](../rust-toolchain.toml).

| Área | Tecnologia | Implementação |
|---|---|---|
| Desktop | GPUI via `gpui-kit` | [crates/app](../crates/app/). Shell nativo, editor DBML, renderização e integração com o sistema |
| Workbench web | TypeScript, HTML/CSS e SVG | [packages/canvas-web](../packages/canvas-web/). Interface compartilhada entre a CLI Web e o VS Code |
| Ponte WebAssembly | `wasm-bindgen`, `serde-wasm-bindgen` | [crates/dbml-wasm](../crates/dbml-wasm/). Análise, edição e cena do diagrama |
| Parsing e análise | `logos`, `rowan` | [crates/core](../crates/core/). Lexer, CST sem perdas, diagnósticos, patches e geometria |
| CLI Web | `clap`, Axum, Tokio, `notify`, `rust-embed` | [crates/cli](../crates/cli/). Binário, servidor local, WebSocket, monitoramento de arquivo e bundle embutido |
| VS Code | API de editor customizado, esbuild | [vscode-extension](../vscode-extension/). Webview, árvore de arquivos e persistência pelo host |
| Exportação | SVG, `resvg` no desktop, APIs do navegador na Web | [diagram-export](../crates/diagram-export/) e [export.ts](../packages/canvas-web/src/export.ts) |
| SQL e bancos | Importação de DDL e leitura de catálogos | [sql-import](../crates/sql-import/) e [db-introspect](../crates/db-introspect/) |
| Tipografia | Geist e Geist Mono | Fontes embarcadas no [desktop](../crates/app/assets/fonts/) e na [Web](../packages/canvas-web/src/fonts/) |

O Vite gera os bundles do workbench, e o Vitest roda seus testes. O núcleo
Rust usa testes unitários e de integração, snapshots com `insta` e testes
de propriedade com `proptest`.

[Arquitetura](03-arquitetura.md) descreve como os alvos se conectam. O
[guia de contribuição](../CONTRIBUTING.md) reúne dependências de sistema e
comandos de build.
