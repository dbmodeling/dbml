# Protocolo de edição: versão do documento

Vale para o servidor CLI (`dbml run`, WebSocket em `/ws`) e para a extensão VS Code (mensagens
Webview ↔ host). O app desktop (GPUI) edita o buffer em processo, sem protocolo, e não é afetado.

## Problema

Um patch textual é um intervalo de bytes (ou linha/coluna) mais o texto novo. Se o documento mudou
entre o cliente calcular o patch e o host aplicá-lo (watcher, outro cliente, editor nativo, Ctrl+Z),
o intervalo aponta para o lugar errado e corrompe o DBML sem aviso. Conferir só limites do intervalo
não basta.

## Versão do documento

O host mantém um contador inteiro monotônico da versão do **texto** do documento ativo. Mudanças de
layout (sidecar) não contam.

| Host | Versão | Sobe quando |
|---|---|---|
| CLI (`DocState.version`, começa em 1) | contador do servidor, único entre arquivos | `apply_patch` aceito, alteração externa vista pelo watcher, troca de arquivo (`open_file`), `save_content` |
| VS Code | `TextDocument.version` | qualquer alteração do texto (digitação, Ctrl+Z, outro editor, patch do Canvas) |

O cliente guarda a última versão recebida e a devolve como `base_version` em todo `apply_patch` e
`save_document`. O host só aceita se `base_version` for igual à versão atual.

## Mensagens (CLI, WebSocket)

Servidor → cliente (todas com `version`):

- `init` (ao conectar) e `update` (troca de arquivo, cliente atrasado): documento completo
  (`source`, `layout`, `title`, `documentId`, `saved`, `files`, `version`).
- `external_change`: `source`, `documentId`, `version`. O watcher venceu as edições ainda não
  gravadas.
- `patch_rejected`: mesmo corpo de `update` mais `reason`. Enviado só a quem mandou o pedido com
  versão velha, imediatamente antes do `result` de erro.
- `result` (`id`, `error`, `version`): `version` aparece em `apply_patch` e `save_document` bem
  sucedidos e é a versão resultante.

Cliente → servidor (`{type, payload, id}`):

- `apply_patch`: `base_version` (obrigatório), `byte_start`, `byte_end`, `new_text`,
  `expected_source`, `expected_file`.
- `save_document`: `base_version` (obrigatório), `expected_source`, `expected_file`.
- `save_layout`, `open_file`: sem versão (não alteram o texto ou geram `update` próprio).

Sem `base_version` o pedido é inválido (`result` com erro). Versão diferente da atual: nada é
aplicado, nada muda no disco nem na versão, e o cliente recebe `patch_rejected` e `result` com erro
`Document version changed (...)`. `expected_source` continua exigido como segunda defesa: mesma
versão com texto divergente (por exemplo, cliente com estado inventado) também é recusada.

A versão e o patch são checados sob o mesmo lock de escrita do documento, então dois clientes que
partem da mesma versão nunca vencem os dois: o segundo recebe `patch_rejected`.

## Mensagens (VS Code)

Webview → host: `apply_patch` e `save_document` levam `base_version` no nível da mensagem. O host
rejeita (`base_version !== document.version`) sem aplicar nada, reenvia `update` com texto e versão
atuais e responde `result` com erro. `update` e `result` de patch/salvar levam `version`. Texto
idêntico com versão diferente também gera `update` (digitar e desfazer dentro do debounce).

## Modo Comparar: versão base no git HEAD

Pedido de **leitura**, fora do controle de versão do documento: não altera texto, disco nem
`version`, e não gera `result`.

- **CLI (WebSocket)**: cliente → servidor `{type: "git_base", payload: {expected_file}, id}`; o
  servidor responde só a quem pediu com
  `{type: "git_base_result", id, source, error, error_code}`. `expected_file` precisa ser o arquivo
  ativo (mesma checagem de `apply_patch`; o servidor nunca lê um caminho escolhido pelo cliente).
  O servidor roda `git rev-parse --is-inside-work-tree`, `git rev-parse --verify HEAD^{commit}` e
  `git cat-file blob HEAD:./<nome>` na pasta do arquivo, sem shell. Passa pelas mesmas defesas
  (`Host`/`Origin` e token de sessão) das demais mensagens.
- **VS Code**: Webview → host `{command: "git_base", id}`; o host responde
  `{type: "git_base_result", id, source | error + error_code}`. Só arquivos `file:` em workspace
  confiável; o extension host web responde `git_unavailable`.
- **Códigos de erro** (`error_code`, mesmos nos dois hosts; `error` é texto técnico em inglês e a
  Webview traduz a mensagem a partir do código): `git_unavailable`, `not_a_repo`, `no_commits`,
  `untracked` (arquivo novo, ainda fora do `HEAD`), `not_utf8` e `other`.

## Cliente (`packages/canvas-web/src/transport.ts`)

`WebSocketTransport` e `VsCodeTransport` guardam a versão (só avança; mensagem atrasada não a faz
recuar; o `init` de uma conexão nova a substitui, pois o servidor pode ter reiniciado), enviam
`base_version` em cada patch/salvamento e recusam enviar antes de conhecer uma versão. Ao receber
`patch_rejected` (CLI) ou o `update` de rejeição (VS Code) o estado é entregue ao Workbench pelo
callback de documento atualizado: o patch não é aplicado localmente, e o erro do `result` rejeita a
promessa de `sendPatch`, mantendo o aviso visível ao usuário. Edições locais ainda não persistidas
continuam sob a regra existente do Workbench (não são sobrescritas; o aviso pede para exportá-las).

`WebSocketTransport` e `VsCodeTransport` expõem `requestGitBase()`, que resolve com
`{ok: true, source}` ou `{ok: false, code, message}`; só falha de canal (conexão caiu) rejeita. O
modo Comparar (`packages/canvas-web/src/diff/`) usa o resultado com `diff_dbml` do Wasm.

## Fila de edições offline (#75)

Fora do escopo: reconciliar uma fila de patches calculados offline exige reaplicar cada edição sobre
a versão nova (rebase de patches ou replay de comandos semânticos), o que mexe no controle de erro do
Workbench. A versão base entrega a peça necessária: um patch da fila enviado após a reconexão tem
`base_version` velha e é recusado com o estado atual, em vez de aplicado no offset errado.
