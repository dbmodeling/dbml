# 02 — Linguagem de modelagem

## Requisitos para o formato texto

- Legível e escrevível à mão; diff limpo em git.
- Expressa: tabelas, colunas, tipos, PK/FK, nullability, defaults, unique, índices
  (compostos), enums, relações (1-1, 1-N, N-N), notas/documentação, agrupamentos, schemas.
- Ecossistema existente (importar/exportar de outras ferramentas).
- Possível de editar **programaticamente sem destruir** comentários e formatação.

## Opções avaliadas

| Opção | Prós | Contras |
|---|---|---|
| **DBML** (dbml.dbdiagram.io) | Padrão de fato no nicho; dbdiagram/dbdocs/`@dbml/cli` (DBML↔SQL); sintaxe enxuta; já existe `dbml-rs` | Não guarda layout; extensões não-oficiais quebram compatibilidade; spec evolui por fora (Holistics) |
| **Linguagem própria** | Liberdade total (layout, views, metadados, constraints avançadas) | Ninguém conhece; zero ecossistema; custo de design e manutenção de spec |
| **SQL DDL** | Universal; executável | Verboso; dialetos divergem; ruim como fonte de modelagem; e SQL está fora do escopo |
| **Prisma schema** | Popular em TS | Acoplado ao ORM; semântica de relações própria |
| **Mermaid `erDiagram`** | Renderiza em GitHub/Markdown | Muito limitado como fonte (sem enums, índices, defaults) — por isso só **export** |
| **Azimutt AML** | Conciso, pensado para exploração | Nicho menor que DBML |

## Recomendação

**DBML como formato canônico, com o modelo interno independente de formato.**

1. O arquivo `*.dbml` do usuário é **DBML válido e estrito** — abre no dbdiagram sem ajustes.
2. Tudo o que é específico do app (posições, cores customizadas, views, viewport, colapso)
   fica em um **arquivo sidecar** (`*.dbml.layout.json`, JSON), nunca dentro do DBML.
   Assim o DBML continua puro e o layout pode ser ignorado no git se o time quiser.
3. O núcleo trabalha sobre um **modelo semântico próprio** (`Schema`). DBML é o único
   formato de leitura/escrita; **Mermaid `erDiagram`** é um formato apenas de *export*
   (com perdas, ver `03-arquitetura.md`). Sem SQL e sem introspecção de banco.
4. Se no futuro precisarmos de algo que DBML não expressa, o caminho é um **superset
   opcional** ("DBML+") detectado por diretiva, com export para DBML puro — nunca o padrão.

### Por que não usar só `dbml-rs` como parser?

`dbml-rs` (1.0.1) é ótimo para *ler* um arquivo válido e obter um AST, e será usado como
**oráculo de conformidade** nos testes. Mas um editor precisa de mais:

- **Tolerância a erros**: o usuário digita texto inválido o tempo todo; o canvas deve
  continuar mostrando o que é parseável e os diagnósticos devem apontar o local exato.
- **Árvore sintática sem perdas (CST)**: guarda espaços e comentários, permitindo que uma
  edição feita no canvas vire um *patch textual mínimo* em vez de reescrever o arquivo.
- **Spans precisos** para diagnósticos, hover, go-to-definition, rename.
- **Incrementalidade** (reparse rápido a cada tecla em schemas grandes).

Por isso a proposta é um parser próprio no estilo rust-analyzer: `logos` (lexer) +
parser recursivo-descendente com recuperação de erros → CST `rowan` → camada de análise
semântica → `Schema`.

## Cobertura DBML alvo (MVP → depois)

- **MVP**: `Table` (alias, settings, `Note`), colunas com `[pk, not null, unique,
  increment, default, note, ref]`, `Ref` (`<`, `>`, `-`, `<>`) inline e em bloco,
  `Indexes`, `Enum`, `Note`, `Project`, comentários `//` e `/* */`.
- **Depois**: `TableGroup`, `TablePartial`, `schema.table`, settings de ref
  (`delete`/`update`), `headercolor`, notas multilinha, sticky notes.

> A spec oficial de DBML deve ser revisada no início da implementação; a lista acima é
> baseada no conhecimento atual e precisa ser confirmada contra a versão vigente.

## Exemplo

```dbml
// Usuários do sistema
Table users {
  id integer [pk, increment]
  email varchar(255) [unique, not null]
  created_at timestamp [default: `now()`]
}

Table posts {
  id integer [pk, increment]
  author_id integer [ref: > users.id, not null]
  status post_status
}

Enum post_status {
  draft
  published
}
```

`schema.dbml.layout.json` (sidecar, exemplo ilustrativo):

```json
{
  "version": 1,
  "tables": {
    "users": { "x": 120, "y": 80 },
    "posts": { "x": 480, "y": 80, "color": "#7c3aed" }
  },
  "views": [{ "name": "Conteúdo", "tables": ["posts", "users"] }],
  "export": { "title": null, "legend": false, "watermark": null }
}
```

JSON formatado de forma estável (chaves ordenadas, 2 espaços) para diffs limpos no git.


## Estado atual implementado

- **Parser e helpers (`crates/dbml-syntax`):** Lexer `logos` + CST `rowan` sem perdas e sem `unsafe`, expondo também `unquote`, `child_tokens` e `descendant_tokens` compartilhados.
- **Análise e edição (`crates/dbml-analysis`, `crates/dbml-edit`):** `analyze`, `lint`, `complete` (com proteção de limites UTF-8 e `proptest`), `Patch` e `EditError` (`Display` + `std::error::Error`).
- **Recursos DBML e sidecar:** Suporte a `Table`, colunas (`pk`, `not null`, `unique`, `ref`), `Ref` (`<`, `>`, `-`, `<>`), `TableGroup` (`color`), `headercolor`, extração de `Enum` e sidecar determinístico `*.dbml.layout.json`.
