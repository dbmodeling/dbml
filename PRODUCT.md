# Product

<!-- impeccable:product-schema 1 -->

> Registro derivado de `docs/01-visao.md`, `docs/05-ux.md`, `docs/identidade-visual.md` e `CLAUDE.md`, a pedido do usuário ("criar a partir do que já existe no docs"). Não houve entrevista; o que não está nos docs aparece como decisão em aberto.

## Platform

web

<!-- A interface roda em três modalidades com o mesmo workbench: Desktop nativo (GPUI), CLI Web (`dbml run`) e extensão VS Code (webview). O esquema do Impeccable só conhece web/ios/android/adaptive; "web" é o mais próximo, pois o Desktop usa identidade própria e não a linguagem nativa de cada SO. -->

## Users

Devs backend/fullstack que desenham um schema novo ou documentam um existente, e tech leads/arquitetos que discutem modelagem (export de imagem, docs, apresentação, revisão em PR).

## Product Purpose

Ecossistema de modelagem de banco de dados no estilo dbdiagram: o schema é um documento DBML versionável e o canvas permite modelar inteiro sem tocar no texto. As duas visões são a mesma coisa, e editar uma atualiza a outra preservando comentários e formatação. Sucesso é modelar na velocidade do pensamento, sem perder trabalho.

## Positioning

Nem "texto primeiro" (canvas só como preview) nem "UI primeiro" (diálogos modais, visual datado, ruim de versionar em git). Edição bidirecional sem perdas entre texto e canvas, teclado primeiro, local-first e rápido (120 fps no Desktop com GPU).

## Operating Context

- Alvos: Windows 10/11, Ubuntu 22.04/24.04, Fedora 40/41 e macOS (Apple Silicon e Intel).
- Modalidades: App Desktop (GPUI), extensão VS Code (`CustomTextEditor` + Wasm) e CLI Web (`dbml run`, Axum/WebSocket local).
- Persistência: texto DBML como fonte da verdade e sidecar `<schema>.dbml.layout.json` para posições e estilos.
- Fluxos: criar/editar no diagrama, editar DBML com diagnósticos, importar SQL DDL, conectar a um banco para engenharia reversa (somente leitura), exportar PNG, JPEG, SVG e Mermaid `erDiagram`.

## Capabilities and Constraints

- DBML do usuário permanece DBML válido; metadados do app vão no sidecar.
- Edições da UI viram patches textuais mínimos.
- Toda ação de UI: acessível por teclado, presente na command palette, sem modal quando for do fluxo principal, e com undo.
- Paridade obrigatória entre Desktop, CLI Web e VS Code para funcionalidades, comandos e UX.
- Zero magic strings e zero magic numbers: valores centralizados em constantes.
- Idioma: pt-BR em conversa, docs e UI de origem, com tradução en na interface.
- Conexão a bancos é somente leitura (só o catálogo, nunca os dados); Oracle ainda sem introspector.
- Em aberto: colaboração em tempo real e versão mobile ficaram fora de escopo.

## Brand Commitments

Nome "DB Modeling". Marca: duas tabelas brancas ligadas por uma relação sobre quadrado arredondado laranja `#e56731`. O mesmo laranja destaca ações e seleções na interface. Tipografia Geist (interface) e Geist Mono (DBML e dados). Detalhes em `docs/identidade-visual.md`.

## Evidence on Hand

- Capturas do workbench em `docs/img/`.
- Schema de exemplo em `crates/app/assets/sample.dbml`.
- Marca em `crates/app/assets/brand/`.
- Não há depoimentos, clientes nem métricas de uso nos docs, e nada disso deve ser inventado.

## Product Principles

1. Zero modais para o fluxo principal; a edição acontece no lugar.
2. Tudo tem atalho e está na command palette; o mouse é opcional.
3. Feedback instantâneo (menos de 16 ms nas interações).
4. Nunca perder trabalho: autosave, recuperação e undo universal.
5. Defaults inteligentes sem esconder poder; descoberta progressiva.

## Accessibility & Inclusion

Teclado completo em toda ação e leitura por leitor de tela nos controles da Web. Não há padrão formal (WCAG) declarado nos docs: decisão em aberto.
