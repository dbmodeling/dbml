# 01 — Visão do produto

## Problema

Ferramentas de modelagem de banco hoje caem em dois extremos:

- **Texto primeiro** (dbdiagram.io, dbdocs, Mermaid): rápidas para quem conhece a sintaxe,
  mas o diagrama é só um "preview" — layout manual frágil, pouca interação, web-only.
- **UI primeiro** (MySQL Workbench, pgModeler, DBeaver ER, Lucidchart): poderosas mas
  lentas, cheias de diálogos modais, datadas visualmente e ruins de versionar em git.

Ninguém entrega bem as duas coisas ao mesmo tempo: **um texto legível e versionável**
*e* **um canvas onde dá para modelar inteiro sem tocar no texto**, com a fluidez de um
editor como o Zed ou de uma ferramenta como Figma/Linear.

## Proposta

**DB Modeling** — um app desktop nativo para **Windows e Ubuntu** onde:

1. O **schema** é um documento texto (DBML), amigável a git e diff.
2. O **canvas** é um cidadão de primeira classe: criar tabela, coluna, tipo, relação,
   índice e enum *apenas pela UI*, com teclado ou mouse, sem modais.
3. As duas visões são **a mesma coisa**: editar uma atualiza a outra instantaneamente,
   preservando comentários e formatação do texto.
4. Tudo é **local-first**, abre instantaneamente e roda a 120 fps mesmo com centenas de tabelas.

## Público

- Devs backend/fullstack desenhando um schema novo ou documentando um existente.
- Tech leads/arquitetos discutindo modelagem (export de imagem, docs, apresentação).

## Diferenciais pretendidos

| Diferencial | Por quê importa |
|---|---|
| Edição bidirecional sem perdas (texto ↔ canvas) | Ninguém precisa escolher entre "modo código" e "modo visual" |
| Teclado primeiro + command palette | Modelar na velocidade do pensamento |
| Edição inline no canvas (sem diálogos) | Menos troca de contexto |
| Sugestões inteligentes (`user_id` → FK para `users.id`, `id` → PK, `created_at` → `timestamp`) | Menos digitação repetitiva |
| Auto-layout de qualidade + roteamento de arestas | Diagramas legíveis sem arrastar caixinha por horas |
| Múltiplas *views* do mesmo schema (subconjuntos/áreas) | Schemas grandes ficam navegáveis |
| Diagnósticos em tempo real (FK para coluna inexistente, tipo incompatível, nome duplicado) | Erros visíveis no texto *e* no canvas |
| Export PNG/JPEG de alta qualidade e Mermaid `erDiagram` | Levar o diagrama para docs, PRs, wikis e apresentações |
| Nativo, offline, rápido | Vantagem estrutural de GPUI vs apps web/Electron |

## Princípios de UX (guiam toda decisão)

1. **Zero modais para o fluxo principal.** Edição acontece no lugar (inline/popover).
2. **Tudo tem atalho; tudo está na command palette.** Mouse é opcional, nunca obrigatório.
3. **Feedback instantâneo** (< 16 ms para interações, < 100 ms para operações pesadas).
4. **Undo universal**: um único histórico cobre edições de texto e de canvas.
5. **Nunca perder trabalho**: autosave, recuperação, e o texto do usuário (comentários,
   ordem, formatação) nunca é reescrito sem necessidade.
6. **Defaults inteligentes, sem esconder poder**: o simples é óbvio, o avançado é alcançável.
7. **Descoberta progressiva**: dicas contextuais de atalho, empty states que ensinam.

## Fora de escopo (por ora)

- Colaboração em tempo real multiusuário (talvez no futuro via CRDT).
- Versão web / mobile (GPUI tem alvo wasm, mas não é prioridade).
- Ferramenta de administração de banco (queries, dados) — somos modelagem.
- **Conexão com banco**: sem leitura/introspecção de banco existente.
- **SQL**: sem export/import de DDL nem migrações. O foco é exclusivamente o visual da modelagem.
- macOS (GPUI suporta, mas não é alvo; pode ser avaliado no futuro).


## Estado atual implementado

- **3 alvos sincronizados:** App Desktop nativo (`crates/app`), CLI Web Server (`crates/cli` / `dbml`) e Extensão VS Code (`vscode-extension/`), compartilhando o núcleo Rust e `packages/canvas-web` via `crates/dbml-wasm`.
- **Plataformas:** Windows, Linux (Ubuntu/Fedora) e macOS, com instaladores (`.exe`, `.dmg`/`.pkg`, `.deb`, `.rpm` e `scripts/install.sh`).
- **Exports:** PNG, JPEG, SVG e Mermaid `erDiagram`.
