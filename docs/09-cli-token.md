# Token de sessão do `dbml run`

A cada execução, o servidor (`crates/cli`) gera um token aleatório de 128 bits (32 caracteres
hexadecimais). `/ws` e `/api/*` respondem `401` sem ele; os assets estáticos continuam livres e
nunca contêm o token.

## Como o token chega ao cliente

- O terminal imprime (e o navegador abre) `http://HOST:PORTA/?token=<token>`.
- Formas aceitas pelo servidor em `/ws` e `/api/*`: query `?token=<token>`, cabeçalho
  `X-DBML-Token: <token>` ou `Authorization: Bearer <token>`. O WebSocket do navegador só consegue
  usar a query string.
- Se `--host` não for loopback, o terminal mostra um aviso: quem tiver a URL com o token pode ler
  e editar os arquivos do workspace.

## Ponte temporária no servidor

Enquanto `packages/canvas-web` não envia o token por conta própria, `crates/cli/src/assets.rs`
injeta no `index.html` um pequeno script (`TOKEN_SHIM`) que lê `?token=` da URL, guarda em
`sessionStorage`, remove o parâmetro da barra de endereço e anexa o token a `new WebSocket(...)`
(`/ws?token=`) e a `fetch('/api/...')` (`X-DBML-Token`) dirigidos ao próprio servidor.

## Pendência no front (`packages/canvas-web`)

- Ler `new URLSearchParams(location.search).get('token')` (ou `sessionStorage`) e anexar o token
  ao abrir o WebSocket (`/ws?token=`) e nas chamadas `fetch('/api/introspect*')`
  (`X-DBML-Token`).
- Depois disso, remover `TOKEN_SHIM` e `inject_token_shim` de `crates/cli/src/assets.rs`.
