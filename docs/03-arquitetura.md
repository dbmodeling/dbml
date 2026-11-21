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

## Estrutura do monorepo

```text
crates/
  core/
    schema/         # modelo semântico
    dbml-syntax/    # lexer e parser tolerante, com CST sem perdas
    dbml-analysis/  # projeção do CST, diagnósticos e autocomplete
    dbml-edit/      # comandos semânticos que produzem patches textuais
    layout/         # posicionamento, roteamento e geometria
  db-introspect/    # leitura de esquemas de bancos de dados
  sql-import/       # conversão de DDL para DBML
  diagram-export/   # exportação de diagramas
  dbml-wasm/        # ponte do Rust para o navegador e o VS Code
  app/             # aplicativo desktop GPUI
  cli/             # binário dbml, servidor HTTP e WebSocket
packages/
  canvas-web/      # canvas e workbench compartilhados pela CLI Web e pelo VS Code
vscode-extension/  # editor customizado, sidebar e integração com o VS Code
```

Os crates em `crates/core/` tratam do modelo, do texto DBML e da geometria. O
desktop os consome diretamente. O canvas compartilhado acessa o núcleo via
`dbml-wasm`. Só o crate `app` depende de GPUI.

O script `./scripts/sync-build.sh` compila a ponte Wasm e atualiza o bundle
embutido em `crates/cli/web/` e o bundle em `vscode-extension/dist/`.

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

1. `layout` fornece a geometria compartilhada com o canvas, incluindo as
   dimensões dos cards e as rotas das arestas.
2. `diagram-export` gera **SVG** e rasteriza com `resvg`/`tiny-skia` na escala
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
