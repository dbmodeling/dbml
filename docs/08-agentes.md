# 08 — Agentes e roteamento de modelos

Em vez de rodar tudo no Opus, a sessão principal funciona como **orquestradora**: entende o
pedido, decide e delega cada atividade a um subagente com o modelo adequado. Os agentes ficam
em `.claude/agents/` (versionados com o projeto).

## Modelos disponíveis (preço por milhão de tokens, entrada / saída)

| Modelo | Preço | Perfil |
|---|---|---|
| Haiku 5.5 | $0,10 / $0,50 | Rápido e muito barato; busca, tarefas mecânicas |
| Sonnet 5.5 | $2 / $10 | Bom custo-benefício para codar e pesquisar |
| Opus 5.5 | $4 / $20 | Raciocínio forte; design, revisão, bugs difíceis (modelo da sessão principal) |

Fonte: tabela de modelos da skill `claude-api` (preços da API, cache de 2026-10-06). Num plano
de assinatura a proporção de consumo de cota segue a mesma ordem.

## Agentes

| Agente | Modelo / esforço | Ferramentas | Quando usar |
|---|---|---|---|
| `explorador` | Haiku · low | somente leitura | Achar arquivos, símbolos, usos, "como funciona hoje" |
| `mecanico` | Haiku · low | edição | Mudança mecânica já especificada: renomear, fmt/clippy --fix, atualizar refs, traduzir, boilerplate, mensagem de commit |
| `pesquisador` | Sonnet · medium | web + escrita em `docs/pesquisa/` | API do GPUI/gpui-kit, versões de crates, specs DBML/Mermaid; a skill `research` |
| `implementador` | Sonnet · high | edição + Bash + Skill | Implementar ticket/tarefa já especificado, com TDD |
| `revisor` | Opus · medium | somente leitura + Bash | Revisão de código; revisores paralelos do `code-review` |

**Por que não há agente Opus de design nem de depuração:** a sessão principal já roda em Opus.
Um subagente Opus custaria o mesmo e começaria do zero, sem a conversa. Por isso design, ADRs,
revisão de plano e bugs difíceis ficam na sessão principal. O `revisor` é a exceção, porque roda
em paralelo (um eixo por instância).

## Regras de roteamento (para a sessão principal)

1. **Delegue por padrão** o trabalho que cabe num agente acima; a sessão principal fica com
   conversa com o usuário, decisões, entrevistas (`grill-me`/`grill-with-docs`) e síntese.
2. **Comece barato e escale com motivo**:
   - busca → `explorador`; se a resposta pede julgamento → a sessão principal decide;
   - implementação → `implementador` (Sonnet); escale passando `model: opus` **e
     `effort: medium`** na chamada (senão ele herda o `high` do frontmatter) quando a tarefa
     tocar `dbml-edit`/CST, o canvas do GPUI, ou quando o implementador falhar duas vezes ou
     devolver bloqueio;
   - bug que o implementador não resolveu → a sessão principal investiga.
3. **Opus sempre em `medium`.** Opus com esforço `high` (ou acima) **nunca** é usado por
   iniciativa própria: só quando a tarefa já foi tentada em Opus `medium` sem sucesso **e** o
   usuário pedir explicitamente o `high` — aí, `effort: high` na chamada daquela tarefa.
   Sonnet e Haiku seguem o esforço do frontmatter.
4. **Skills que disparam subagentes** usam estes agentes: `research` → `pesquisador`;
   `code-review` → `revisor` (cada eixo); `implement`/`implement-spec` → `implementador`;
   `improve-codebase-architecture` → sessão principal; varreduras de código → `explorador`.
5. **O subagente não vê a conversa**: o prompt precisa trazer o objetivo, os arquivos/tickets
   relevantes, as decisões já tomadas e o formato de resposta esperado.
6. **Paralelize** tarefas independentes (várias buscas, vários tickets sem bloqueio entre si)
   em chamadas simultâneas; tarefas que dependem uma da outra, em sequência.
7. **Quando NÃO delegar.** Cada subagente começa sem contexto e precisa reler os arquivos.
   Faça direto quando:
   - você já sabe qual arquivo e qual trecho mudar (até ~3 arquivos conhecidos);
   - a busca cabe em 1–2 `grep`/`glob` com termo exato;
   - a edição tem menos de ~20 linhas e já está decidida;
   - a tarefa depende do que foi discutido na conversa e não cabe num prompt curto.
   Delegue quando a busca for aberta (vários termos, vários crates), a implementação passar de
   ~3 arquivos ou de um ciclo de TDD, ou o resultado gerar muito texto (logs, pesquisa) que
   poluiria o contexto principal.
8. **Sincronia entre Desktop, CLI Web e VS Code:** Sempre que uma tarefa alterar comandos,
   regras do núcleo (`schema`, `dbml-syntax`, `dbml-analysis`, `dbml-edit`, `layout`, `dbml-wasm`)
   ou comportamento de UI/Canvas (`crates/app`, `packages/canvas-web`, `vscode-extension`),
   inclua no prompt do `implementador` e do `revisor` a exigência de atualizar as 3 modalidades e
   executar `./scripts/sync-build.sh` ao final.

## Ajustes

- Trocar o modelo de um agente: editar `model:`/`effort:` no frontmatter do arquivo em
  `.claude/agents/`.
- Forçar um modelo para todos os subagentes (ex.: economizar numa sessão):
  `CLAUDE_CODE_SUBAGENT_MODEL=haiku` com `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` no `env` do
  `.claude/settings.local.json`.
- A sessão principal pode rodar num modelo mais barato com `/model sonnet`; nesse caso o
  o `revisor` continua em Opus pelo frontmatter. Nesse modo, design e bugs difíceis podem ir
  para um agente com `model: opus` e `effort: medium` na chamada.
- Os hooks de caveman e ponytail também se aplicam aos subagentes (`SubagentStart`).

## Outras ferramentas: Codex e Antigravity (`agy`)

Os agentes de `.claude/agents/` são exclusivos do Claude Code. O que vale para as três ferramentas:

| Item | Claude Code | Codex | Antigravity (`agy`) |
|---|---|---|---|
| Regras do projeto | `CLAUDE.md` | `AGENTS.md` | `AGENTS.md` |
| MCP context7 + Playwright | `.mcp.json` (versionado) | `~/.codex/config.toml` (global, `codex mcp add`) | `~/.gemini/config/mcp_config.json` (global) |
| Skills `/check`, `/release` | `.claude/skills` → symlink p/ `.agents/skills/` | `.agents/skills/` | `.agents/skills/` |
| ponytail | plugin | plugin (`codex plugin`) | plugin (`~/.gemini/config/plugins/`) |
| mattpocock-skills, caveman | plugins | `~/.agents/skills/` (global) | `~/.gemini/config/skills/` (global) |
| grill-me, grilling, find-skills, alethe | `~/.claude/skills` → `~/.agents/skills` | `~/.agents/skills/` | symlinks em `~/.gemini/config/skills/` |
| OpenSpec (`/opsx`) | `/opsx:propose` | `$openspec-propose` | `/opsx-propose` |
| Papéis de agente | `.claude/agents/*.md` | seguir os papéis desta página manualmente | idem |

Codex e `agy` não leem MCP de dentro do repositório, então numa máquina nova é preciso
registrar os servidores de novo:

```bash
codex mcp add context7 -- npx -y @upstash/context7-mcp
codex mcp add playwright -- npx -y @playwright/mcp@latest
# agy: copiar o bloco "mcpServers" do .mcp.json para ~/.gemini/config/mcp_config.json
```

Numa máquina nova, para instalar as skills de terceiros no Codex e no Antigravity:

```bash
npx skills add mattpocock/skills -g -a codex -a antigravity -s '*' -y
npx skills add <caminho-ou-repo-do-caveman> -g -a codex -a antigravity -s '*' -y
openspec init --tools claude,codex,antigravity .   # gera .agents/skills/openspec-* e .agents/workflows/
```

Caveman e ponytail só ficam sempre ativos onde estão instalados como plugin com hooks.
No Codex e no Antigravity o caveman é só skill: precisa ser invocado.

O Playwright só serve para as interfaces web (`dbml run` e `packages/canvas-web` em
`localhost`). Ele não controla o app desktop GPUI nem a webview da extensão VS Code.
