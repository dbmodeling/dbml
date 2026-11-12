# CLAUDE.md

**DB Modeling** — ecossistema de modelagem de banco de dados (estilo dbdiagram) em
**Rust + GPUI / Web / Wasm**, para **Windows, Linux (Ubuntu/Fedora) e macOS**, com foco máximo em UI/UX e edição
bidirecional texto (DBML) ↔ canvas.

## Escopo
- Dentro: modelagem visual + texto DBML; export **PNG, JPEG, SVG e Mermaid `erDiagram`**.
- Modalidades:
  1. **App Desktop Nativo (GPUI):** 120 fps, aceleração por GPU (DirectX/Vulkan/Metal).
  2. **Extensão VS Code (`.vsix`):** Editor customizado com Webview Canvas e núcleo Wasm/CST local.
  3. **CLI Web Server (`dbml run`):** Servidor HTTP/WebSocket local em Axum com auto-reload e sincronização contínua.
- Alvos: Windows 10/11, Ubuntu 22.04/24.04, Fedora 40/41 e macOS (Apple Silicon e Intel).
- Persistência: Texto DBML como fonte da verdade persistida + sidecar JSON (`<schema>.dbml.layout.json`).

## Status
Workspace multi-crate integrado e funcional:
- `schema`: modelo semântico puro com `serde` habilitado.
- `dbml-syntax` & `dbml-analysis`: CST Rowan sem perdas, helpers compartilhados (`unquote`, `child_tokens`, `descendant_tokens`) e linter/autocomplete semântico.
- `dbml-edit`: patches textuais mínimos sobre o buffer (`Patch`, `EditError`).
- `layout`: auto-layout em camadas/clusterizado (Sugiyama), roteamento ortogonal de arestas e métricas/paleta canônicas compartilhadas (`CARD_W`, `HEADER_H`, `ROW_H`, `GROUP_PAD`, `TABLE_COLORS`, `compute_diagram_positions`).
- `dbml-wasm`: compilação Wasm (`wasm32-unknown-unknown`) e ponte UTF-16 (`LineIndex`) para Web e VS Code.
- `cli` (`dbml-cli`): utilitário CLI e servidor Axum/WebSocket local com assets embutidos e escrita atômica.
- `app`: aplicação desktop GPUI modularizada (`shell/mod.rs`, `shell/panels.rs`, `canvas.rs`, `canvas_geom.rs`, `session.rs`, `settings.rs`).
- `packages/canvas-web`: motor interativo de Canvas 2D, pan/zoom, edição inline e testes em Vitest.
- `vscode-extension`: extensão oficial VS Code (`CustomTextEditor` + Wasm bundle embutido).
- Instaladores / Wizards: Windows Inno Setup (`.exe`), macOS (`.dmg`/`.pkg`), Debian/Ubuntu (`.deb`), Fedora (`.rpm`) e `scripts/install.sh`.

## Fluxo de trabalho
**Prioridade: skills do `mattpocock-skills`.** Quando uma skill dele cobre o mesmo
terreno de outra, use a dele:

| Etapa | Usar | Em vez de |
|---|---|---|
| Afinar ideia/design | `/grill-with-docs` (grilling + domain-modeling) ou `/grill-me` | `/opsx:explore` |
| Spec | `/to-spec` → `.scratch/<feature>/spec.md` | `/opsx:propose` |
| Quebrar em tarefas | `/to-tickets` → `.scratch/<feature>/issues/` | tasks do OpenSpec |
| Implementar | `/implement` (usa `tdd` e fecha com `code-review`) | `/opsx:apply` |
| Revisão | `mattpocock-skills:code-review` | `/code-review` nativo |
| Bugs difíceis | `diagnosing-bugs` (+ `defense-in-depth` na correção) | — |

OpenSpec (`/opsx:*`) continua disponível só quando o usuário pedir
explicitamente. Plugins de estilo ativos: caveman (chat curto; arquivos em prosa normal) e
ponytail (menor solução completa). Detalhes em `docs/07-skills.md`.

## Agentes e modelos
A sessão principal **orquestra e delega por padrão** (o usuário pediu isso explicitamente) a
agentes de `.claude/agents/` com modelo fixo. Detalhes e regras de escalonamento em
`docs/08-agentes.md`.

| Atividade | Agente | Modelo |
|---|---|---|
| Buscar/localizar no código | `explorador` | Haiku |
| Mudança mecânica já especificada | `mecanico` | Haiku |
| Pesquisa em docs/código de crates (`research`) | `pesquisador` | Sonnet |
| Implementar ticket/tarefa (`implement`) | `implementador` | Sonnet (escalar p/ Opus `medium` em `dbml-edit`/CST, canvas, ou após 2 falhas) |
| Design, ADRs, revisar plano | `arquiteto` | Opus · medium |
| Revisão de código (`code-review`) | `revisor` | Opus · medium |
| Bug difícil / performance | `depurador` | Opus · medium |

**Opus nunca em `high`** (ou acima) por iniciativa própria: só se a tarefa falhou em Opus
`medium` **e** o usuário pedir o `high` explicitamente. Ao escalar um agente Sonnet para
Opus, passar `effort: medium` na chamada. Fable só com pedido explícito do usuário. Conversa, entrevistas (`grill-me`) e decisões ficam
na sessão principal. Todo prompt de subagente leva objetivo, arquivos, decisões e formato de
resposta — ele não vê esta conversa.

## Agent skills

### Issue tracker
Markdown local em `.scratch/<feature>/` (ver `docs/agents/issue-tracker.md`).

### Triage labels
Os cinco papéis canônicos traduzidos (`precisa-triagem`, `precisa-info`, `pronto-para-agente`, `pronto-para-humano`, `nao-sera-feito`). Ver `docs/agents/triage-labels.md`.

### Domain docs
Contexto único: `GLOSSARY.md` na raiz + ADRs em `openspec/adr/` (nunca `docs/adr/`). Ver `docs/agents/domain.md`.

## Idioma
**Tudo em português (pt-BR)**: conversa, docs, ADRs, glossário, specs, tickets em
`.scratch/`, design docs, skills do projeto, mensagens de commit e descrições de PR. Exceções: identificadores de código, nomes de crates/arquivos,
comandos, e palavras-chave estruturais do OpenSpec.

## Princípios que não mudam sem discussão
- DBML do usuário permanece **DBML válido**; metadados do app vão no sidecar de layout.
- Edições da UI viram **patches textuais mínimos** (preservar comentários/formatação).
- Núcleo desacoplado de UI; crates puras e testáveis headless.
- Toda ação de UI: acessível por teclado, presente na command palette, sem modal, com undo.
- **Sincronia obrigatória Desktop + CLI Web + VS Code:** Qualquer mudança em funcionalidades, comandos semânticos ou UX/UI deve ser sincronizada nas 3 modalidades (`crates/app`, `packages/canvas-web` + `crates/cli`, e `vscode-extension` + `crates/dbml-wasm`). Sempre rodar `./scripts/sync-build.sh` após alterações no núcleo ou no `packages/canvas-web`.

## Stack (fixar versões exatas)
- **Desktop:** `gpui-kit` =0.8.0 (→ `gpui-pre` =0.3.9, `gpui-component` =0.8.0), `logos` 0.16.1, `rowan` 0.17.0, `petgraph` 0.8.3, `kurbo` 0.13.1, `resvg` 0.48.1, `tiny-skia` 0.12.0.
- **Web & Wasm:** `wasm-bindgen` 0.2.129, `serde-wasm-bindgen` 0.6, `axum` 0.8, `tokio` 1.x, `tokio-tungstenite` 0.29, `rust-embed` 8.x.
- **Frontend:** TypeScript 5.7, Vite 6.x, Vitest 5.x, esbuild 0.25.
- A API do GPUI muda muito: consultar o código-fonte da versão fixada (docs.rs / repositório) em vez de confiar em exemplos antigos.

## Comandos Rápidos
- Sincronizar Tudo (Wasm + Canvas Web + CLI + VS Code + Desktop): `./scripts/sync-build.sh` (`--package` para `.vsix`, `--release` para binários release)
- Desktop: `cargo run -p app`
- CLI Web: `cargo run -p dbml-cli -- run schema.dbml`
- Testes Rust: `cargo test --workspace`
- Testes Web Canvas: `cd packages/canvas-web && npm test`
- Build Extensão VS Code: `cd vscode-extension && npm run build && npm run package`
- Instalador Universal: `./scripts/install.sh`
