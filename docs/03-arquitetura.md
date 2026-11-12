# 03 — Arquitetura

## Visão geral

```
            ┌─────────────── app (GPUI) ────────────────┐
            │  Editor DBML     Canvas     Inspector     │
            │      │             │            │         │
            │      └──── comandos / eventos ──┘         │
            └──────────────────┬────────────────────────┘
                               ▼
                        ┌─────────────┐
                        │  Workspace  │  documento + histórico (undo) + layout
                        └──────┬──────┘
          texto ──parse──▶ CST (rowan) ──análise──▶ Schema (modelo semântico)
          texto ◀─patch─── edição de CST ◀─comando── UI
```

## Workspace de crates (proposta)

```
crates/
  schema/        # modelo semântico puro: Table, Column, Type, Ref, Index, Enum, ids estáveis
  dbml-syntax/   # lexer (logos) + parser tolerante → CST rowan; sem dependência de UI
  dbml-analysis/ # CST → Schema, diagnósticos, resolução de nomes, completions, rename
  dbml-edit/     # operações semânticas → patches textuais mínimos sobre o CST
  scene/         # descrição visual pura (caixas, textos, arestas, cores) a partir de Schema + layout
  export/        # scene → SVG → PNG/JPEG (resvg/tiny-skia + image); Schema → Mermaid erDiagram
  layout/        # auto-layout (camadas/Sugiyama), roteamento de arestas, geometria
  project/       # arquivo .dbml + sidecar de layout, autosave, file watching
  app/           # binário GPUI: janelas, canvas, editor, painéis, temas, atalhos
```

Regra: **só `app` depende de GPUI.** Todo o resto é Rust puro, testável com `cargo test`
e snapshot (`insta`), e reaproveitável (CLI, LSP standalone).

Plataformas alvo: **Windows 10/11** (GPUI usa DirectX) e **Ubuntu 22.04/24.04** (Vulkan;
Wayland e X11). Atalhos usam `Ctrl`.

## Fonte da verdade e sincronização

A decisão central do projeto. **O texto DBML é a fonte da verdade persistida; o CST é a
fonte da verdade em memória; o `Schema` é uma projeção derivada.**

### Texto → modelo (usuário digita no editor)
1. Edição altera o buffer de texto.
2. Reparse (incremental quando possível) → novo CST.
3. Análise → novo `Schema` + diagnósticos.
4. Diff de `Schema` antigo × novo por **identidade estável** → canvas anima só o que mudou.

### UI → texto (usuário edita no canvas)
1. UI emite um **comando semântico** (`AddColumn { table, after, name, ty }`,
   `RenameTable`, `CreateRef`…).
2. `dbml-edit` localiza os nós no CST e gera um **patch textual mínimo** (inserção de
   uma linha, troca de um token), respeitando a indentação/estilo existentes.
3. O patch é aplicado ao buffer como qualquer outra edição → cai no fluxo acima.

Vantagem: existe **um único caminho de mudança** (edição de texto), logo undo/redo,
autosave, file watching e diagnósticos são unificados. Comentários e formatação do
usuário nunca são perdidos.

### Identidade estável
Tabelas/colunas recebem ids internos que sobrevivem a reparse (casamento por nome +
posição + heurística de rename). O sidecar de layout referencia **nomes**; renames
feitos pela UI atualizam o sidecar atomicamente. Renames feitos no texto são inferidos
(ex.: tabela sumiu e apareceu outra no mesmo lugar com as mesmas colunas).

## Canvas

- Elemento GPUI customizado (`Element`/`canvas`) que desenha **arestas** (paths bezier ou
  ortogonais) e grid; **tabelas** como elementos GPUI posicionados (texto, foco, hover,
  edição inline reaproveitam componentes).
- **Zoom/pan**: GPUI não tem transformações afins gerais em toda a árvore; o canvas
  calcula posições/tamanhos/fonte escalados. Abaixo de um limiar de zoom → **LOD**
  (só cabeçalhos, depois só retângulos).
- **Culling**: só monta elementos visíveis no viewport (+ margem).
- Hit-testing próprio para arestas e handles de conexão.
- Minimap, snapping a grid/guias, seleção múltipla por retângulo.

## Export

### PNG / JPEG
GPUI não oferece renderização off-screen para imagem de forma simples, então o export
não "fotografa" a janela. Em vez disso:

1. `scene` gera uma descrição visual (retângulos, textos, paths das arestas, cores do
   tema) a partir do `Schema` + layout — **a mesma geometria usada pelo canvas**.
2. `export` converte a cena em **SVG** e rasteriza com `resvg`/`tiny-skia` na escala
   escolhida (1×/2×/4×), usando **as mesmas fontes embarcadas** no app.
3. PNG (com ou sem fundo transparente) ou JPEG (fundo sólido obrigatório, qualidade
   configurável) via crate `image`.
4. Título, legenda e marca d'água opcionais são elementos da própria cena (mesmo
   pipeline, mesmas fontes).

Ganho colateral: export SVG fica praticamente pronto, caso queiramos oferecer depois.
Risco: paridade visual canvas × export (ver R8 em `riscos.md`).

### Mermaid `erDiagram`
Tradução direta do `Schema` (layout é ignorado — Mermaid faz o próprio):

| DBML | Mermaid |
|---|---|
| `Table users { … }` | `users { … }` |
| coluna `email varchar(255) [unique, not null]` | `varchar email UK` (tipo sanitizado) |
| `[pk]` / FK / `[unique]` | `PK` / `FK` / `UK` (combináveis) |
| `note` da coluna | comentário `"…"` do atributo |
| `Ref: posts.author_id > users.id` (FK not null) | `users \|\|--o{ posts : "author_id"` |
| FK anulável | lado "um" vira `\|o` (zero ou um) |
| `1-1`, `N-N` | `\|\|--\|\|`, `}o--o{` |
| enums, índices, defaults, TableGroups, cores | **sem equivalente** → avisados no painel de export (enum pode virar comentário) |

Nomes/tipos com caracteres que Mermaid não aceita são sanitizados de forma
determinística. Validar a saída contra o parser oficial do Mermaid nos testes (CLI
`mmdc`, opcional no CI).

## Undo/redo
Histórico de transações sobre o buffer de texto, com metadados (origem, seleção no
canvas, posições de layout) para que desfazer uma ação de UI restaure também a seleção
e o layout. Mudanças só de layout (mover tabela) entram no mesmo histórico.

## Persistência
- `schema.dbml` + `schema.dbml.layout.json` lado a lado (um sidecar por schema; JSON com
  ordem de chaves estável).
- Autosave + arquivo de recuperação; detecção de mudança externa (`notify`) com merge
  (reaplicar como edição de texto).
- Configurações do usuário em `directories::ProjectDirs`.

## Testabilidade
- Parser/análise/edição: testes de snapshot (`insta`) + corpus de arquivos DBML reais +
  conformidade com `dbml-rs`.
- Propriedade-chave testada com `proptest`: *para todo comando de UI, aplicar o patch e
  reparsear produz o `Schema` esperado* e *texto sem mudança semântica é preservado byte a byte*.
- UI: testes headless do GPUI (`TestAppContext`) para fluxos críticos.


## Estado atual dos crates e módulos

```
crates/
  schema/        # modelo semântico puro (#![forbid(unsafe_code)], #![deny(missing_docs)])
  dbml-syntax/   # lexer logos + parser CST rowan + helpers compartilhados (unquote, child_tokens, descendant_tokens)
  dbml-analysis/ # CST → Schema, lint e complete (seguro em UTF-8 + proptest)
  dbml-edit/     # patches textuais mínimos (Patch, EditError)
  layout/        # auto-layout Sugiyama (layered/clustered), roteamento ortogonal e geometria/paleta compartilhadas (CARD_W, HEADER_H, ROW_H, GROUP_PAD, TABLE_COLORS, compute_diagram_positions)
  dbml-wasm/     # bindings WebAssembly + LineIndex UTF-16 + cena para Web e VS Code
  cli/           # binário dbml (`dbml-cli`): servidor Axum/WebSocket, file watcher e escrita atômica
  app/           # binário desktop GPUI: shell/{mod.rs,panels.rs}, canvas.rs, canvas_geom.rs, session.rs, settings.rs, glass.rs, actions.rs
```

- **Frontend Web compartilhado:** `packages/canvas-web` (TypeScript + Canvas 2D) embutido em `crates/cli/web/` e `vscode-extension/`, sincronizado via `./scripts/sync-build.sh`.
- **Segurança e lints:** Os 6 crates de biblioteca usam `#![forbid(unsafe_code)]` e `#![deny(missing_docs)]`; o workspace aplica `clippy::pedantic` e `-D clippy::perf`.
