# Riscos e perguntas em aberto

## Riscos técnicos

| # | Risco | Impacto | Mitigação |
|---|---|---|---|
| R1 | Desenvolvimento em **WSL2**: GPUI no Linux usa Vulkan e o WSLg pode falhar ou ficar lento; o build Windows exige toolchain Windows | Ciclo de desenvolvimento | Spike 1; testar no WSLg; compilar/rodar o alvo Windows nativamente (rustup no Windows) ou com `cargo-xwin`; CI com matriz Windows + Ubuntu desde o dia 1 |
| R2 | GPUI não tem transformações gerais → zoom/pan exige escalar layout manualmente; texto em zoom baixo | Qualidade do canvas | Spike 2; LOD; culling; elemento customizado |
| R3 | API do editor do `gpui-component` para diagnósticos/completion próprios pode ser limitada | Editor de texto | Spike 3; fallback: editor mais simples + painel de diagnósticos |
| R4 | Patches mínimos texto↔UI são difíceis (estilo, indentação, casos de borda) | Coração do produto | Spike 4; testes de propriedade; fallback: reformatar só o bloco da tabela afetada |
| R5 | Churn de API do GPUI/gpui-kit | Manutenção | Versões fixadas, upgrades em lote, GPUI isolado no `app` |
| R6 | Auto-layout e roteamento de arestas de qualidade são problemas de pesquisa | UX de schemas grandes | Começar com Sugiyama simples; evoluir; layout manual sempre possível |
| R7 | Evolução da spec DBML por terceiros | Compatibilidade | Corpus de testes; acompanhar changelog do `@dbml/core` |
| R8 | Paridade visual entre canvas (GPUI) e export (SVG/resvg): medidas de texto, antialiasing, fontes | Export "não parece o app" | Geometria única no crate `scene`; mesmas fontes embarcadas; medição de texto compartilhada; testes de snapshot de imagem |
| R9 | Mermaid `erDiagram` tem sintaxe restrita (nomes, tipos com parênteses, cardinalidades) | Export Mermaid inválido | Sanitização determinística; validar com `mmdc` nos testes; listar perdas no painel de export |
| R10 | Diferenças Windows × Ubuntu (DPI/escala fracionária, fontes, IME, Wayland vs X11, clipboard de imagem) | Bugs específicos de plataforma | Testes manuais nos dois alvos a cada fase; CI nos dois |

## Decidido
- Nome: **DB Modeling**.
- Alvos: **Windows e Ubuntu** (sem macOS por ora).
- Sem leitura de banco e sem SQL; exports **PNG, JPEG e Mermaid**.
- Sidecar de layout em **JSON**, um por schema (`schema.dbml.layout.json`).
- App abre **"canvas first"** (editor de texto fechado por padrão).
- Versões suportadas: **Windows 10 e 11**, **Ubuntu 22.04 e 24.04**.
- Export de imagem com **título, legenda e marca d'água opcionais**.

## Perguntas em aberto

1. **Licença** do código: open source (qual?) ou fechado/proprietário?


## Atualizações de status

- **Licença definida:** MIT ([LICENSE](file:///home/smovisk/Documents/db_modeling/LICENSE) e `Cargo.toml`).
- **Alvos expandidos:** Além de Windows e Ubuntu, o projeto agora inclui **Fedora**, **macOS**, **CLI Web** e **Extensão VS Code**.
- **Paridade (R8):** Métricas de cartão, paleta (`TABLE_COLORS`) e auto-layout ficam centralizados no crate `layout` e são sincronizados nos 3 alvos via `./scripts/sync-build.sh`.
